mod client;
mod errors;
mod limits;
mod policy;
mod resolver;

pub use errors::{RemoteStreamError, RemoteStreamErrorKind};
pub use policy::RemoteTargetDiagnostics;

use client::{
    validate_head_probe_response, validate_upstream_response, RemoteHttpClient,
    ValidatedUpstreamResponse, DEFAULT_RESPONSE_HEADER_TIMEOUT,
};
use errors::{invalid_request, response_invalid, source_unavailable, target_forbidden};
use limits::{RemoteStreamLimits, RemoteStreamPermit};
use mova_scan::{
    read_http_strm_reference, HttpStrmReference, ReadHttpStrmReferenceError, StrmReferenceError,
};
use policy::RemoteTargetPolicy;
use reqwest::{header, StatusCode, Url};
use resolver::{RemoteDnsResolver, SystemDnsResolver};
use std::{fmt, path::Path, sync::Arc};

const MAX_REDIRECTS: usize = 3;
const MAX_RANGE_HEADER_BYTES: usize = 256;
const MAX_IF_RANGE_HEADER_BYTES: usize = 1024;

#[derive(Clone)]
pub struct StrmStreamingConfig {
    allowed_hosts: Option<String>,
    user_agent: String,
}

impl StrmStreamingConfig {
    pub fn new(build_version: &str, allowed_hosts: Option<&str>) -> Result<Self, &'static str> {
        // Parse eagerly so startup fails on unsafe or ambiguous configuration.
        RemoteTargetPolicy::from_allowed_hosts(allowed_hosts)?;
        let build_version = build_version.trim();
        if build_version.is_empty() {
            return Err("the Mova build version cannot be empty");
        }
        Ok(Self {
            allowed_hosts: allowed_hosts
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned),
            user_agent: format!("mova/{build_version}"),
        })
    }
}

impl fmt::Debug for StrmStreamingConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StrmStreamingConfig")
            .field(
                "private_allowlist_configured",
                &self.allowed_hosts.is_some(),
            )
            .field("user_agent", &self.user_agent)
            .finish()
    }
}

#[derive(Clone)]
pub struct StrmStreamingService {
    policy: RemoteTargetPolicy,
    resolver: Arc<dyn RemoteDnsResolver>,
    client: RemoteHttpClient,
    limits: RemoteStreamLimits,
}

impl StrmStreamingService {
    pub fn new(config: StrmStreamingConfig) -> Result<Self, &'static str> {
        Ok(Self {
            policy: RemoteTargetPolicy::from_allowed_hosts(config.allowed_hosts.as_deref())?,
            resolver: Arc::new(SystemDnsResolver::new(DEFAULT_RESPONSE_HEADER_TIMEOUT)),
            client: RemoteHttpClient::new(&config.user_agent)?,
            limits: RemoteStreamLimits::default(),
        })
    }

    pub async fn open_carrier(
        &self,
        carrier_path: &Path,
        user_id: i64,
        request: RemoteStreamRequest,
    ) -> Result<RemoteStreamResponse, RemoteStreamError> {
        // Admission covers every potentially blocking stage, including a
        // carrier hosted on a slow network mount, DNS and response headers.
        let permit = self.limits.try_acquire(user_id)?;
        let carrier_path = carrier_path.to_path_buf();
        let reference = tokio::task::spawn_blocking(move || {
            read_http_strm_reference(&carrier_path).map_err(map_reference_error)
        })
        .await
        .map_err(|_| {
            RemoteStreamError::new(
                RemoteStreamErrorKind::CarrierReadFailed,
                "the STRM carrier could not be read",
            )
        })??;

        self.open_reference(reference, request, permit).await
    }

    async fn open_reference(
        &self,
        reference: HttpStrmReference,
        request: RemoteStreamRequest,
        permit: RemoteStreamPermit,
    ) -> Result<RemoteStreamResponse, RemoteStreamError> {
        let mut current_url: Url = reference.url;
        let mut redirects = 0usize;
        let mut wire_request = request.clone();
        let mut head_probe = false;

        loop {
            let target = self
                .policy
                .validate(
                    current_url.clone(),
                    &reference.reference_hash,
                    self.resolver.as_ref(),
                )
                .await?;
            let response = self.client.send(&target, &wire_request).await?;

            if is_redirect(response.status()) {
                if redirects >= MAX_REDIRECTS {
                    return Err(source_unavailable());
                }
                current_url = resolve_redirect(&current_url, response.headers())?;
                redirects += 1;
                continue;
            }

            if wire_request.method() == RemoteStreamMethod::Head
                && matches!(
                    response.status(),
                    StatusCode::METHOD_NOT_ALLOWED | StatusCode::NOT_IMPLEMENTED
                )
            {
                wire_request = RemoteStreamRequest::probe();
                head_probe = true;
                continue;
            }

            let validated = if head_probe {
                validate_head_probe_response(response)?
            } else {
                validate_upstream_response(response, &wire_request)?
            };
            return Ok(RemoteStreamResponse::from_validated(
                validated,
                if request.method() == RemoteStreamMethod::Head {
                    None
                } else {
                    Some(permit)
                },
                target.diagnostics,
            ));
        }
    }
}

impl Default for StrmStreamingService {
    fn default() -> Self {
        Self::new(
            StrmStreamingConfig::new(env!("CARGO_PKG_VERSION"), None)
                .expect("package version must be a valid STRM client version"),
        )
        .expect("default STRM service configuration must be valid")
    }
}

fn map_reference_error(error: ReadHttpStrmReferenceError) -> RemoteStreamError {
    match error {
        ReadHttpStrmReferenceError::Invalid(StrmReferenceError::TooLarge) => {
            RemoteStreamError::new(
                RemoteStreamErrorKind::ReferenceTooLarge,
                "the STRM carrier exceeds the maximum allowed size",
            )
        }
        ReadHttpStrmReferenceError::Invalid(_) => RemoteStreamError::new(
            RemoteStreamErrorKind::ReferenceInvalid,
            "the STRM carrier no longer contains a valid HTTP or HTTPS reference",
        ),
        ReadHttpStrmReferenceError::Io(error) if error.kind() == std::io::ErrorKind::NotFound => {
            RemoteStreamError::new(
                RemoteStreamErrorKind::CarrierNotFound,
                "the STRM carrier no longer exists",
            )
        }
        ReadHttpStrmReferenceError::Io(_) => RemoteStreamError::new(
            RemoteStreamErrorKind::CarrierReadFailed,
            "the STRM carrier could not be read",
        ),
    }
}

fn is_redirect(status: StatusCode) -> bool {
    matches!(
        status,
        StatusCode::MOVED_PERMANENTLY
            | StatusCode::FOUND
            | StatusCode::SEE_OTHER
            | StatusCode::TEMPORARY_REDIRECT
            | StatusCode::PERMANENT_REDIRECT
    )
}

fn resolve_redirect(
    current_url: &Url,
    headers: &header::HeaderMap,
) -> Result<Url, RemoteStreamError> {
    let location = exactly_one_header(headers, header::LOCATION)?
        .ok_or_else(response_invalid)?
        .to_str()
        .map_err(|_| response_invalid())?;
    let next_url = current_url.join(location).map_err(|_| response_invalid())?;
    if current_url.scheme() == "https" && next_url.scheme() == "http" {
        return Err(target_forbidden());
    }
    Ok(next_url)
}

fn exactly_one_header(
    headers: &header::HeaderMap,
    name: header::HeaderName,
) -> Result<Option<&header::HeaderValue>, RemoteStreamError> {
    let mut values = headers.get_all(&name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(response_invalid());
    }
    Ok(first)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteStreamMethod {
    Get,
    Head,
}

#[derive(Clone)]
pub struct RemoteStreamRequest {
    method: RemoteStreamMethod,
    range: Option<RemoteByteRange>,
    if_range: Option<header::HeaderValue>,
}

impl RemoteStreamRequest {
    pub fn new(
        method: RemoteStreamMethod,
        range: Option<&str>,
        if_range: Option<&str>,
    ) -> Result<Self, RemoteStreamError> {
        let range = range.map(RemoteByteRange::parse).transpose()?;
        let if_range = if range.is_some() {
            if_range
                .map(|value| {
                    if value.is_empty() || value.len() > MAX_IF_RANGE_HEADER_BYTES {
                        return Err(invalid_request("invalid If-Range header"));
                    }
                    header::HeaderValue::from_str(value)
                        .map_err(|_| invalid_request("invalid If-Range header"))
                })
                .transpose()?
        } else {
            None
        };
        Ok(Self {
            method,
            range,
            if_range,
        })
    }

    fn probe() -> Self {
        Self::new(RemoteStreamMethod::Get, Some("bytes=0-0"), None)
            .expect("static probe range must be valid")
    }

    pub fn method(&self) -> RemoteStreamMethod {
        self.method
    }

    pub(crate) fn range(&self) -> Option<&RemoteByteRange> {
        self.range.as_ref()
    }

    pub(crate) fn if_range(&self) -> Option<&header::HeaderValue> {
        self.if_range.as_ref()
    }
}

#[derive(Clone)]
pub(crate) struct RemoteByteRange {
    raw: header::HeaderValue,
    kind: RemoteByteRangeKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RemoteByteRangeKind {
    FromTo { start: u64, end: u64 },
    From { start: u64 },
    Suffix { length: u64 },
}

impl RemoteByteRange {
    fn parse(value: &str) -> Result<Self, RemoteStreamError> {
        if value.is_empty() || value.len() > MAX_RANGE_HEADER_BYTES || !value.is_ascii() {
            return Err(invalid_request("invalid Range header"));
        }
        let specification = value
            .strip_prefix("bytes=")
            .ok_or_else(|| invalid_request("unsupported Range header"))?;
        if specification.contains(',') {
            return Err(invalid_request("multiple byte ranges are not supported"));
        }
        let (start, end) = specification
            .split_once('-')
            .ok_or_else(|| invalid_request("invalid Range header"))?;
        let kind = match (start.is_empty(), end.is_empty()) {
            (true, true) => return Err(invalid_request("invalid Range header")),
            (true, false) => {
                let length = parse_positive_range_number(end)?;
                RemoteByteRangeKind::Suffix { length }
            }
            (false, true) => RemoteByteRangeKind::From {
                start: parse_range_number(start)?,
            },
            (false, false) => {
                let start = parse_range_number(start)?;
                let end = parse_range_number(end)?;
                if end < start {
                    return Err(invalid_request("invalid Range header"));
                }
                RemoteByteRangeKind::FromTo { start, end }
            }
        };
        Ok(Self {
            raw: header::HeaderValue::from_str(value)
                .map_err(|_| invalid_request("invalid Range header"))?,
            kind,
        })
    }

    pub(crate) fn as_header_value(&self) -> &header::HeaderValue {
        &self.raw
    }

    pub(crate) fn kind(&self) -> RemoteByteRangeKind {
        self.kind
    }

    pub(crate) fn explicitly_starts_at_zero(&self) -> bool {
        matches!(
            self.kind,
            RemoteByteRangeKind::From { start: 0 } | RemoteByteRangeKind::FromTo { start: 0, .. }
        )
    }
}

fn parse_range_number(value: &str) -> Result<u64, RemoteStreamError> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(invalid_request("invalid Range header"));
    }
    value
        .parse::<u64>()
        .map_err(|_| invalid_request("invalid Range header"))
}

fn parse_positive_range_number(value: &str) -> Result<u64, RemoteStreamError> {
    let value = parse_range_number(value)?;
    if value == 0 {
        Err(invalid_request("invalid Range header"))
    } else {
        Ok(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteResponseHeaderName {
    ContentType,
    ContentLength,
    ContentRange,
    AcceptRanges,
    Etag,
    LastModified,
}

impl RemoteResponseHeaderName {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ContentType => "content-type",
            Self::ContentLength => "content-length",
            Self::ContentRange => "content-range",
            Self::AcceptRanges => "accept-ranges",
            Self::Etag => "etag",
            Self::LastModified => "last-modified",
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct RemoteResponseHeader {
    name: RemoteResponseHeaderName,
    value: Vec<u8>,
}

impl fmt::Debug for RemoteResponseHeader {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RemoteResponseHeader")
            .field("name", &self.name)
            .field("value_length", &self.value.len())
            .finish()
    }
}

impl RemoteResponseHeader {
    fn new(name: RemoteResponseHeaderName, value: Vec<u8>) -> Self {
        Self { name, value }
    }

    pub fn name(&self) -> RemoteResponseHeaderName {
        self.name
    }

    pub fn value(&self) -> &[u8] {
        &self.value
    }
}

pub struct RemoteStreamResponse {
    status: u16,
    headers: Vec<RemoteResponseHeader>,
    body: Option<RemoteStreamBody>,
}

impl RemoteStreamResponse {
    fn from_validated(
        validated: ValidatedUpstreamResponse,
        permit: Option<RemoteStreamPermit>,
        diagnostics: RemoteTargetDiagnostics,
    ) -> Self {
        let body = validated
            .response
            .zip(permit)
            .map(|(response, permit)| RemoteStreamBody {
                response,
                _permit: permit,
                diagnostics,
            });
        Self {
            status: validated.status,
            headers: validated.headers,
            body,
        }
    }

    pub fn status(&self) -> u16 {
        self.status
    }

    pub fn headers(&self) -> &[RemoteResponseHeader] {
        &self.headers
    }

    pub fn into_body(self) -> Option<RemoteStreamBody> {
        self.body
    }
}

pub struct RemoteStreamBody {
    response: reqwest::Response,
    _permit: RemoteStreamPermit,
    diagnostics: RemoteTargetDiagnostics,
}

impl RemoteStreamBody {
    pub async fn next_chunk(&mut self) -> Result<Option<bytes::Bytes>, RemoteStreamBodyFailure> {
        self.response.chunk().await.map_err(|error| {
            if error.is_timeout() {
                RemoteStreamBodyFailure::TimedOut
            } else {
                RemoteStreamBodyFailure::Upstream
            }
        })
    }

    pub fn diagnostics(&self) -> &RemoteTargetDiagnostics {
        &self.diagnostics
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteStreamBodyFailure {
    TimedOut,
    Upstream,
}

impl RemoteStreamBodyFailure {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TimedOut => "body_idle_timeout",
            Self::Upstream => "upstream_body_failure",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::net::SocketAddr;
    use std::{
        io::{Read, Write},
        sync::atomic::{AtomicUsize, Ordering},
        time::Duration,
    };

    struct StaticResolver(SocketAddr);

    struct CountingResolver {
        address: SocketAddr,
        calls: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl RemoteDnsResolver for StaticResolver {
        async fn resolve(
            &self,
            _hostname: &str,
            port: u16,
        ) -> Result<Vec<SocketAddr>, resolver::DnsResolutionError> {
            let mut address = self.0;
            address.set_port(port);
            Ok(vec![address])
        }
    }

    #[async_trait]
    impl RemoteDnsResolver for CountingResolver {
        async fn resolve(
            &self,
            _hostname: &str,
            port: u16,
        ) -> Result<Vec<SocketAddr>, resolver::DnsResolutionError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let mut address = self.address;
            address.set_port(port);
            Ok(vec![address])
        }
    }

    fn mock_service(address: SocketAddr) -> StrmStreamingService {
        mock_service_with_client(address, RemoteHttpClient::new("mova/test").unwrap())
    }

    fn mock_service_with_client(
        address: SocketAddr,
        client: RemoteHttpClient,
    ) -> StrmStreamingService {
        StrmStreamingService {
            policy: RemoteTargetPolicy::unsafe_allow_loopback_for_mock_upstream(),
            resolver: Arc::new(StaticResolver(address)),
            client,
            limits: RemoteStreamLimits::default(),
        }
    }

    fn read_request(socket: &mut std::net::TcpStream) -> String {
        let mut request = Vec::new();
        let mut buffer = [0u8; 1024];
        while !request.windows(4).any(|window| window == b"\r\n\r\n") {
            let length = socket.read(&mut buffer).unwrap();
            if length == 0 {
                break;
            }
            request.extend_from_slice(&buffer[..length]);
        }
        String::from_utf8_lossy(&request).to_string()
    }

    #[test]
    fn range_parser_accepts_single_ranges_and_rejects_ambiguous_input() {
        for value in ["bytes=0-0", "bytes=1-", "bytes=-500"] {
            RemoteStreamRequest::new(RemoteStreamMethod::Get, Some(value), None).unwrap();
        }
        for value in [
            "items=0-1",
            "bytes=",
            "bytes=0-1,3-4",
            "bytes=4-3",
            "bytes=-0",
            "bytes= 0-1",
        ] {
            assert!(RemoteStreamRequest::new(RemoteStreamMethod::Get, Some(value), None).is_err());
        }
        let oversized = format!("bytes=0-{}", "1".repeat(MAX_RANGE_HEADER_BYTES));
        assert!(RemoteStreamRequest::new(RemoteStreamMethod::Get, Some(&oversized), None).is_err());
    }

    #[tokio::test]
    async fn admission_limit_is_checked_before_reading_the_carrier() {
        let service = StrmStreamingService {
            limits: RemoteStreamLimits::new(1, 1),
            ..Default::default()
        };
        let held = service.limits.try_acquire(42).unwrap();
        let missing_carrier =
            std::env::temp_dir().join(format!("mova-strm-missing-{}.strm", uuid::Uuid::new_v4()));

        let error = match service
            .open_carrier(
                &missing_carrier,
                42,
                RemoteStreamRequest::new(RemoteStreamMethod::Get, None, None).unwrap(),
            )
            .await
        {
            Ok(_) => panic!("a saturated user must be rejected before carrier I/O"),
            Err(error) => error,
        };

        assert_eq!(error.kind(), RemoteStreamErrorKind::UserLimitExceeded);
        drop(held);
    }

    #[test]
    fn redirects_support_relative_locations_and_forbid_https_downgrades() {
        let mut headers = header::HeaderMap::new();
        headers.insert(
            header::LOCATION,
            header::HeaderValue::from_static("../final.mp4"),
        );
        let redirected = resolve_redirect(
            &Url::parse("https://media.example/dir/start").unwrap(),
            &headers,
        )
        .unwrap();
        assert_eq!(redirected.as_str(), "https://media.example/final.mp4");

        headers.insert(
            header::LOCATION,
            header::HeaderValue::from_static("http://media.example/final.mp4"),
        );
        let error = resolve_redirect(
            &Url::parse("https://media.example/start").unwrap(),
            &headers,
        )
        .unwrap_err();
        assert_eq!(error.kind(), RemoteStreamErrorKind::TargetForbidden);
    }

    #[test]
    fn strm_configuration_debug_never_reveals_private_allowlist_hosts() {
        let config = StrmStreamingConfig::new("1.2.3", Some("secret-nas.home:5244")).unwrap();
        let rendered = format!("{config:?}");

        assert!(rendered.contains("private_allowlist_configured"));
        assert!(!rendered.contains("secret-nas.home"));
        assert!(StrmStreamingConfig::new("1.2.3", Some("*.home:443")).is_err());
    }

    #[tokio::test]
    async fn dedicated_client_does_not_forward_sensitive_request_headers() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let request = read_request(&mut socket);
            socket
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: video/mp4\r\nContent-Length: 2\r\nSet-Cookie: upstream=secret\r\nConnection: close\r\n\r\nok",
                )
                .unwrap();
            request
        });

        let carrier =
            std::env::temp_dir().join(format!("mova-strm-client-{}.strm", uuid::Uuid::new_v4()));
        std::fs::write(
            &carrier,
            format!(
                "http://mock-upstream.invalid:{}/video.mp4?token=secret",
                address.port()
            ),
        )
        .unwrap();
        let service = mock_service(address);
        let request =
            RemoteStreamRequest::new(RemoteStreamMethod::Get, Some("bytes=0-"), Some("\"etag\""))
                .unwrap();
        let response = service.open_carrier(&carrier, 9, request).await.unwrap();
        assert_eq!(response.status(), 200);
        assert!(!response
            .headers()
            .iter()
            .any(|header| header.name().as_str() == "set-cookie"));
        drop(response);

        let request = server.join().unwrap().to_ascii_lowercase();
        assert!(request.contains("range: bytes=0-"));
        assert!(request.contains("if-range: \"etag\""));
        assert!(request.contains("accept-encoding: identity"));
        assert!(request.contains("user-agent: mova/test"));
        assert!(request.contains(&format!("host: mock-upstream.invalid:{}", address.port())));
        assert!(!request.contains("authorization:"));
        assert!(!request.contains("cookie:"));
        assert!(!request.contains("x-forwarded-"));
        let _ = std::fs::remove_file(carrier);
    }

    #[tokio::test]
    async fn head_falls_back_to_a_zero_byte_get_probe() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut head_socket, _) = listener.accept().unwrap();
            let head = read_request(&mut head_socket);
            head_socket
                .write_all(
                    b"HTTP/1.1 405 Method Not Allowed\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .unwrap();

            let (mut probe_socket, _) = listener.accept().unwrap();
            let probe = read_request(&mut probe_socket);
            probe_socket
                .write_all(
                    b"HTTP/1.1 206 Partial Content\r\nContent-Type: video/mp4\r\nContent-Range: bytes 0-0/500\r\nContent-Length: 1\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\nx",
                )
                .unwrap();
            (head, probe)
        });

        let carrier =
            std::env::temp_dir().join(format!("mova-strm-head-{}.strm", uuid::Uuid::new_v4()));
        std::fs::write(
            &carrier,
            format!("http://mock-upstream.invalid:{}/video.mp4", address.port()),
        )
        .unwrap();
        let response = mock_service(address)
            .open_carrier(
                &carrier,
                10,
                RemoteStreamRequest::new(RemoteStreamMethod::Head, None, None).unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        assert!(response.into_body().is_none());

        let (head, probe) = server.join().unwrap();
        assert!(head.starts_with("HEAD /video.mp4 HTTP/1.1"));
        assert!(probe.starts_with("GET /video.mp4 HTTP/1.1"));
        assert!(probe.to_ascii_lowercase().contains("range: bytes=0-0"));
        let _ = std::fs::remove_file(carrier);
    }

    #[tokio::test]
    async fn redirect_limit_is_three_and_dns_is_revalidated_for_every_hop() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            for hop in 0..4 {
                let (mut socket, _) = listener.accept().unwrap();
                let request = read_request(&mut socket);
                assert!(request.starts_with(&format!("GET /hop{hop} HTTP/1.1")));
                socket
                    .write_all(
                        format!(
                            "HTTP/1.1 302 Found\r\nLocation: /hop{}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                            hop + 1
                        )
                        .as_bytes(),
                    )
                    .unwrap();
            }
        });

        let calls = Arc::new(AtomicUsize::new(0));
        let service = StrmStreamingService {
            policy: RemoteTargetPolicy::unsafe_allow_loopback_for_mock_upstream(),
            resolver: Arc::new(CountingResolver {
                address,
                calls: calls.clone(),
            }),
            client: RemoteHttpClient::new("mova/test").unwrap(),
            limits: RemoteStreamLimits::default(),
        };
        let carrier =
            std::env::temp_dir().join(format!("mova-strm-redirect-{}.strm", uuid::Uuid::new_v4()));
        std::fs::write(
            &carrier,
            format!("http://mock-upstream.invalid:{}/hop0", address.port()),
        )
        .unwrap();

        let error = match service
            .open_carrier(
                &carrier,
                11,
                RemoteStreamRequest::new(RemoteStreamMethod::Get, None, None).unwrap(),
            )
            .await
        {
            Ok(_) => panic!("a fourth redirect must be rejected"),
            Err(error) => error,
        };
        assert_eq!(error.kind(), RemoteStreamErrorKind::SourceUnavailable);
        assert_eq!(calls.load(Ordering::SeqCst), 4);
        server.join().unwrap();
        let _ = std::fs::remove_file(carrier);
    }

    #[tokio::test]
    async fn response_header_timeout_maps_without_retaining_the_upstream_error() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let _request = read_request(&mut socket);
            std::thread::sleep(Duration::from_millis(150));
        });
        let carrier = std::env::temp_dir().join(format!(
            "mova-strm-header-timeout-{}.strm",
            uuid::Uuid::new_v4()
        ));
        std::fs::write(
            &carrier,
            format!("http://mock-upstream.invalid:{}/video", address.port()),
        )
        .unwrap();
        let client = RemoteHttpClient::with_timeouts(
            "mova/test",
            Duration::from_secs(1),
            Duration::from_millis(40),
            Duration::from_secs(1),
        );
        let error = match mock_service_with_client(address, client)
            .open_carrier(
                &carrier,
                12,
                RemoteStreamRequest::new(RemoteStreamMethod::Get, None, None).unwrap(),
            )
            .await
        {
            Ok(_) => panic!("a stalled response header must time out"),
            Err(error) => error,
        };
        assert_eq!(error.kind(), RemoteStreamErrorKind::SourceTimeout);
        assert!(!format!("{error:?} {error}").contains("mock-upstream"));
        server.join().unwrap();
        let _ = std::fs::remove_file(carrier);
    }

    #[tokio::test]
    async fn body_idle_timeout_is_per_read_and_body_drop_releases_its_permit() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let _request = read_request(&mut socket);
            socket
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: video/mp4\r\nContent-Length: 2\r\nConnection: close\r\n\r\n",
                )
                .unwrap();
            std::thread::sleep(Duration::from_millis(150));
            let _ = socket.write_all(b"ok");
        });
        let carrier = std::env::temp_dir().join(format!(
            "mova-strm-body-timeout-{}.strm",
            uuid::Uuid::new_v4()
        ));
        std::fs::write(
            &carrier,
            format!("http://mock-upstream.invalid:{}/video", address.port()),
        )
        .unwrap();
        let client = RemoteHttpClient::with_timeouts(
            "mova/test",
            Duration::from_secs(1),
            Duration::from_secs(1),
            Duration::from_millis(40),
        );
        let service = mock_service_with_client(address, client);
        let limits = service.limits.clone();
        let response = service
            .open_carrier(
                &carrier,
                13,
                RemoteStreamRequest::new(RemoteStreamMethod::Get, None, None).unwrap(),
            )
            .await
            .unwrap();
        let mut body = response.into_body().unwrap();
        assert_eq!(
            body.next_chunk().await.unwrap_err(),
            RemoteStreamBodyFailure::TimedOut
        );

        let held = (0..limits::DEFAULT_PER_USER_STREAM_LIMIT - 1)
            .map(|_| limits.try_acquire(13).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            limits.try_acquire(13).unwrap_err().kind(),
            RemoteStreamErrorKind::UserLimitExceeded
        );
        drop(body);
        let replacement = limits.try_acquire(13).unwrap();
        drop((held, replacement));

        server.join().unwrap();
        let _ = std::fs::remove_file(carrier);
    }

    #[tokio::test]
    async fn truncated_upstream_body_becomes_a_sanitized_stream_failure() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let _request = read_request(&mut socket);
            socket
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: video/mp4\r\nContent-Length: 10\r\nConnection: close\r\n\r\nok",
                )
                .unwrap();
        });
        let carrier =
            std::env::temp_dir().join(format!("mova-strm-truncated-{}.strm", uuid::Uuid::new_v4()));
        std::fs::write(
            &carrier,
            format!("http://mock-upstream.invalid:{}/video", address.port()),
        )
        .unwrap();
        let response = mock_service(address)
            .open_carrier(
                &carrier,
                14,
                RemoteStreamRequest::new(RemoteStreamMethod::Get, None, None).unwrap(),
            )
            .await
            .unwrap();
        let mut body = response.into_body().unwrap();
        let failure = loop {
            match body.next_chunk().await {
                Ok(Some(_)) => continue,
                Ok(None) => panic!("a truncated Content-Length must not look complete"),
                Err(failure) => break failure,
            }
        };
        assert_eq!(failure, RemoteStreamBodyFailure::Upstream);

        server.join().unwrap();
        let _ = std::fs::remove_file(carrier);
    }
}
