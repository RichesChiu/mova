use super::{
    errors::{
        response_invalid, source_timeout, source_unavailable, RemoteStreamError,
        RemoteStreamErrorKind,
    },
    policy::ValidatedRemoteTarget,
    RemoteByteRange, RemoteByteRangeKind, RemoteResponseHeader, RemoteResponseHeaderName,
    RemoteStreamMethod, RemoteStreamRequest,
};
use reqwest::{
    header::{self, HeaderMap, HeaderValue},
    redirect::Policy,
    Client, Method, Response, StatusCode,
};
use std::{fmt, time::Duration};

pub(crate) const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(8);
pub(crate) const DEFAULT_RESPONSE_HEADER_TIMEOUT: Duration = Duration::from_secs(15);
pub(crate) const DEFAULT_BODY_IDLE_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone)]
pub(crate) struct RemoteHttpClient {
    user_agent: HeaderValue,
    connect_timeout: Duration,
    response_header_timeout: Duration,
    body_idle_timeout: Duration,
}

pub(crate) struct ValidatedUpstreamResponse {
    pub(crate) status: u16,
    pub(crate) headers: Vec<RemoteResponseHeader>,
    pub(crate) response: Option<Response>,
}

impl fmt::Debug for ValidatedUpstreamResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ValidatedUpstreamResponse")
            .field("status", &self.status)
            .field("header_count", &self.headers.len())
            .field("has_body", &self.response.is_some())
            .finish()
    }
}

impl RemoteHttpClient {
    pub(crate) fn new(user_agent: &str) -> Result<Self, &'static str> {
        let user_agent = HeaderValue::from_str(user_agent)
            .map_err(|_| "the STRM User-Agent contains invalid HTTP header bytes")?;
        Ok(Self {
            user_agent,
            connect_timeout: DEFAULT_CONNECT_TIMEOUT,
            response_header_timeout: DEFAULT_RESPONSE_HEADER_TIMEOUT,
            body_idle_timeout: DEFAULT_BODY_IDLE_TIMEOUT,
        })
    }

    #[cfg(test)]
    pub(crate) fn with_timeouts(
        user_agent: &str,
        connect_timeout: Duration,
        response_header_timeout: Duration,
        body_idle_timeout: Duration,
    ) -> Self {
        let mut client = Self::new(user_agent).expect("test User-Agent must be valid");
        client.connect_timeout = connect_timeout;
        client.response_header_timeout = response_header_timeout;
        client.body_idle_timeout = body_idle_timeout;
        client
    }

    pub(crate) async fn send(
        &self,
        target: &ValidatedRemoteTarget,
        request: &RemoteStreamRequest,
    ) -> Result<Response, RemoteStreamError> {
        let client = self.client_for_target(target)?;
        let method = match request.method() {
            RemoteStreamMethod::Get => Method::GET,
            RemoteStreamMethod::Head => Method::HEAD,
        };
        let mut upstream = client
            .request(method, target.url.clone())
            .header(header::ACCEPT, "*/*")
            .header(header::ACCEPT_ENCODING, "identity");
        if let Some(range) = request.range() {
            upstream = upstream.header(header::RANGE, range.as_header_value());
            if let Some(if_range) = request.if_range() {
                upstream = upstream.header(header::IF_RANGE, if_range);
            }
        }

        let response = tokio::time::timeout(self.response_header_timeout, upstream.send())
            .await
            .map_err(|_| source_timeout())?
            .map_err(|error| {
                if error.is_timeout() {
                    source_timeout()
                } else {
                    source_unavailable()
                }
            })?;
        Ok(response)
    }

    fn client_for_target(
        &self,
        target: &ValidatedRemoteTarget,
    ) -> Result<Client, RemoteStreamError> {
        let mut builder = Client::builder()
            .no_proxy()
            .redirect(Policy::none())
            .connect_timeout(self.connect_timeout)
            .read_timeout(self.body_idle_timeout)
            .user_agent(self.user_agent.clone())
            .use_rustls_tls();

        // The URL retains the original hostname for HTTP Host and TLS SNI, but
        // the socket may only use the exact address set validated for this hop.
        if target.hostname.parse::<std::net::IpAddr>().is_err() {
            builder = builder.resolve_to_addrs(&target.hostname, &target.addresses);
        }

        builder.build().map_err(|_| source_unavailable())
    }
}

pub(crate) fn validate_upstream_response(
    response: Response,
    request: &RemoteStreamRequest,
) -> Result<ValidatedUpstreamResponse, RemoteStreamError> {
    let status = response.status();
    if status == StatusCode::RANGE_NOT_SATISFIABLE {
        if request.range().is_none() {
            return Err(response_invalid());
        }
        let content_range = parse_unsatisfied_content_range(response.headers())?;
        let headers = vec![RemoteResponseHeader::new(
            RemoteResponseHeaderName::ContentRange,
            content_range.into_bytes(),
        )];
        return Ok(ValidatedUpstreamResponse {
            status: status.as_u16(),
            headers,
            response: None,
        });
    }

    if !matches!(status, StatusCode::OK | StatusCode::PARTIAL_CONTENT) {
        return Err(source_unavailable());
    }

    validate_content_encoding(response.headers())?;
    validate_media_content_type(response.headers())?;
    let mut headers = collect_safe_response_headers(response.headers())?;

    match status {
        StatusCode::PARTIAL_CONTENT => {
            let requested_range = request.range().ok_or_else(response_invalid)?;
            let actual_range = parse_satisfied_content_range(response.headers())?;
            validate_content_range_against_request(actual_range, requested_range)?;
            if let Some(content_length) = parse_optional_content_length(response.headers())? {
                let expected = actual_range
                    .end
                    .checked_sub(actual_range.start)
                    .and_then(|length| length.checked_add(1))
                    .ok_or_else(response_invalid)?;
                if content_length != expected {
                    return Err(response_invalid());
                }
            }
        }
        StatusCode::OK => {
            if one_header(response.headers(), header::CONTENT_RANGE)?.is_some() {
                return Err(response_invalid());
            }
            if request
                .range()
                .is_some_and(|range| !range.explicitly_starts_at_zero())
                && request.if_range().is_none()
            {
                return Err(RemoteStreamError::new(
                    RemoteStreamErrorKind::RangeNotSupported,
                    "the remote source ignored a non-zero byte range",
                ));
            }
            if request.range().is_some() {
                // A zero-start Range may fall back to full playback, but the
                // response must not advertise seek support after ignoring it.
                headers.retain(|header| {
                    header.name() != RemoteResponseHeaderName::AcceptRanges
                        && header.name() != RemoteResponseHeaderName::ContentRange
                });
            }
        }
        _ => unreachable!("status filtered above"),
    }

    Ok(ValidatedUpstreamResponse {
        status: status.as_u16(),
        headers,
        response: Some(response),
    })
}

fn validate_content_encoding(headers: &HeaderMap) -> Result<(), RemoteStreamError> {
    if let Some(value) = one_header(headers, header::CONTENT_ENCODING)? {
        let value = value.to_str().map_err(|_| response_invalid())?;
        if !value.trim().eq_ignore_ascii_case("identity") {
            return Err(response_invalid());
        }
    }
    Ok(())
}

pub(crate) fn validate_head_probe_response(
    response: Response,
) -> Result<ValidatedUpstreamResponse, RemoteStreamError> {
    if response.status() == StatusCode::RANGE_NOT_SATISFIABLE {
        let total = parse_unsatisfied_content_range_total(response.headers())?;
        if total != 0 {
            return Err(response_invalid());
        }
        return Ok(ValidatedUpstreamResponse {
            status: StatusCode::OK.as_u16(),
            headers: vec![
                RemoteResponseHeader::new(RemoteResponseHeaderName::ContentLength, b"0".to_vec()),
                RemoteResponseHeader::new(
                    RemoteResponseHeaderName::AcceptRanges,
                    b"bytes".to_vec(),
                ),
            ],
            response: None,
        });
    }

    let request = RemoteStreamRequest::probe();
    let mut validated = validate_upstream_response(response, &request)?;
    if validated.status == StatusCode::PARTIAL_CONTENT.as_u16() {
        let response = validated.response.as_ref().ok_or_else(response_invalid)?;
        let range = parse_satisfied_content_range(response.headers())?;
        validated.status = StatusCode::OK.as_u16();
        validated.headers.retain(|header| {
            !matches!(
                header.name(),
                RemoteResponseHeaderName::ContentLength
                    | RemoteResponseHeaderName::ContentRange
                    | RemoteResponseHeaderName::AcceptRanges
            )
        });
        if let Some(total) = range.total {
            validated.headers.push(RemoteResponseHeader::new(
                RemoteResponseHeaderName::ContentLength,
                total.to_string().into_bytes(),
            ));
        }
        validated.headers.push(RemoteResponseHeader::new(
            RemoteResponseHeaderName::AcceptRanges,
            b"bytes".to_vec(),
        ));
    }
    validated.response = None;
    Ok(validated)
}

fn validate_media_content_type(headers: &HeaderMap) -> Result<(), RemoteStreamError> {
    let Some(value) = one_header(headers, header::CONTENT_TYPE)? else {
        return Ok(());
    };
    let value = value.to_str().map_err(|_| response_invalid())?;
    let media_type = value
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    if media_type.is_empty() {
        return Err(response_invalid());
    }
    let hls = media_type.contains("mpegurl")
        || matches!(
            media_type.as_str(),
            "application/m3u8" | "audio/m3u8" | "video/m3u8"
        );
    let allowed = media_type.starts_with("video/")
        || media_type.starts_with("audio/")
        || matches!(
            media_type.as_str(),
            "application/octet-stream" | "application/mp4" | "application/x-matroska"
        );
    if hls || !allowed {
        return Err(response_invalid());
    }
    Ok(())
}

fn collect_safe_response_headers(
    headers: &HeaderMap,
) -> Result<Vec<RemoteResponseHeader>, RemoteStreamError> {
    let mut output = Vec::new();
    for (source, target) in [
        (header::CONTENT_TYPE, RemoteResponseHeaderName::ContentType),
        (
            header::CONTENT_LENGTH,
            RemoteResponseHeaderName::ContentLength,
        ),
        (
            header::CONTENT_RANGE,
            RemoteResponseHeaderName::ContentRange,
        ),
        (
            header::ACCEPT_RANGES,
            RemoteResponseHeaderName::AcceptRanges,
        ),
        (header::ETAG, RemoteResponseHeaderName::Etag),
        (
            header::LAST_MODIFIED,
            RemoteResponseHeaderName::LastModified,
        ),
    ] {
        if let Some(value) = one_header(headers, source)? {
            output.push(RemoteResponseHeader::new(target, value.as_bytes().to_vec()));
        }
    }
    parse_optional_content_length(headers)?;
    if let Some(value) = one_header(headers, header::ACCEPT_RANGES)? {
        let value = value.to_str().map_err(|_| response_invalid())?;
        if !matches!(value.trim().to_ascii_lowercase().as_str(), "bytes" | "none") {
            return Err(response_invalid());
        }
    }
    Ok(output)
}

fn one_header(
    headers: &HeaderMap,
    name: header::HeaderName,
) -> Result<Option<&HeaderValue>, RemoteStreamError> {
    let mut values = headers.get_all(&name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(response_invalid());
    }
    Ok(first)
}

fn parse_optional_content_length(headers: &HeaderMap) -> Result<Option<u64>, RemoteStreamError> {
    one_header(headers, header::CONTENT_LENGTH)?
        .map(|value| {
            value
                .to_str()
                .ok()
                .and_then(|value| value.parse::<u64>().ok())
                .ok_or_else(response_invalid)
        })
        .transpose()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SatisfiedContentRange {
    start: u64,
    end: u64,
    total: Option<u64>,
}

fn parse_satisfied_content_range(
    headers: &HeaderMap,
) -> Result<SatisfiedContentRange, RemoteStreamError> {
    let value = one_header(headers, header::CONTENT_RANGE)?
        .ok_or_else(response_invalid)?
        .to_str()
        .map_err(|_| response_invalid())?;
    let value = value.strip_prefix("bytes ").ok_or_else(response_invalid)?;
    let (interval, total) = value.split_once('/').ok_or_else(response_invalid)?;
    let (start, end) = interval.split_once('-').ok_or_else(response_invalid)?;
    let start = start.parse::<u64>().map_err(|_| response_invalid())?;
    let end = end.parse::<u64>().map_err(|_| response_invalid())?;
    if start > end {
        return Err(response_invalid());
    }
    let total = if total == "*" {
        None
    } else {
        let total = total.parse::<u64>().map_err(|_| response_invalid())?;
        if total == 0 || end >= total {
            return Err(response_invalid());
        }
        Some(total)
    };
    Ok(SatisfiedContentRange { start, end, total })
}

fn parse_unsatisfied_content_range(headers: &HeaderMap) -> Result<String, RemoteStreamError> {
    let total = parse_unsatisfied_content_range_total(headers)?;
    Ok(format!("bytes */{total}"))
}

fn parse_unsatisfied_content_range_total(headers: &HeaderMap) -> Result<u64, RemoteStreamError> {
    let value = one_header(headers, header::CONTENT_RANGE)?
        .ok_or_else(response_invalid)?
        .to_str()
        .map_err(|_| response_invalid())?;
    value
        .strip_prefix("bytes */")
        .and_then(|total| total.parse::<u64>().ok())
        .ok_or_else(response_invalid)
}

fn validate_content_range_against_request(
    actual: SatisfiedContentRange,
    requested: &RemoteByteRange,
) -> Result<(), RemoteStreamError> {
    let matches = match requested.kind() {
        RemoteByteRangeKind::FromTo { start, end } => {
            actual.start == start
                && actual.end
                    == actual
                        .total
                        .map(|total| end.min(total.saturating_sub(1)))
                        .unwrap_or(end)
        }
        RemoteByteRangeKind::From { start } => {
            actual.start == start
                && actual
                    .total
                    .is_none_or(|total| actual.end == total.saturating_sub(1))
        }
        RemoteByteRangeKind::Suffix { length } => actual.total.is_some_and(|total| {
            actual.start == total.saturating_sub(length) && actual.end == total.saturating_sub(1)
        }),
    };
    if matches {
        Ok(())
    } else {
        Err(response_invalid())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeout_defaults_match_the_strm_streaming_contract() {
        assert_eq!(DEFAULT_CONNECT_TIMEOUT, Duration::from_secs(8));
        assert_eq!(DEFAULT_RESPONSE_HEADER_TIMEOUT, Duration::from_secs(15));
        assert_eq!(DEFAULT_BODY_IDLE_TIMEOUT, Duration::from_secs(30));
    }

    fn response(status: StatusCode, headers: &[(&str, &str)]) -> Response {
        let mut builder =
            reqwest::Response::from(http::Response::builder().status(status).body("").unwrap());
        for (name, value) in headers {
            builder.headers_mut().insert(
                header::HeaderName::from_bytes(name.as_bytes()).unwrap(),
                HeaderValue::from_str(value).unwrap(),
            );
        }
        builder
    }

    #[test]
    fn accepts_consistent_partial_content() {
        let request =
            RemoteStreamRequest::new(RemoteStreamMethod::Get, Some("bytes=10-19"), None).unwrap();
        let validated = validate_upstream_response(
            response(
                StatusCode::PARTIAL_CONTENT,
                &[
                    ("content-type", "video/mp4"),
                    ("content-range", "bytes 10-19/100"),
                    ("content-length", "10"),
                ],
            ),
            &request,
        )
        .unwrap();
        assert_eq!(validated.status, 206);
    }

    #[test]
    fn rejects_mismatched_partial_content_and_length() {
        let request =
            RemoteStreamRequest::new(RemoteStreamMethod::Get, Some("bytes=10-19"), None).unwrap();
        for headers in [
            vec![("content-range", "bytes 11-19/100")],
            vec![
                ("content-range", "bytes 10-19/100"),
                ("content-length", "9"),
            ],
        ] {
            let error = validate_upstream_response(
                response(StatusCode::PARTIAL_CONTENT, &headers),
                &request,
            )
            .unwrap_err();
            assert_eq!(error.kind(), RemoteStreamErrorKind::ResponseInvalid);
        }
    }

    #[test]
    fn permits_ignored_zero_range_but_rejects_ignored_nonzero_range() {
        let zero =
            RemoteStreamRequest::new(RemoteStreamMethod::Get, Some("bytes=0-"), None).unwrap();
        let validated = validate_upstream_response(
            response(
                StatusCode::OK,
                &[("content-type", "video/mp4"), ("accept-ranges", "bytes")],
            ),
            &zero,
        )
        .unwrap();
        assert!(!validated
            .headers
            .iter()
            .any(|header| header.name() == RemoteResponseHeaderName::AcceptRanges));

        let nonzero =
            RemoteStreamRequest::new(RemoteStreamMethod::Get, Some("bytes=1-"), None).unwrap();
        let error = validate_upstream_response(
            response(StatusCode::OK, &[("content-type", "video/mp4")]),
            &nonzero,
        )
        .unwrap_err();
        assert_eq!(error.kind(), RemoteStreamErrorKind::RangeNotSupported);

        let conditional = RemoteStreamRequest::new(
            RemoteStreamMethod::Get,
            Some("bytes=1-"),
            Some("\"stale-etag\""),
        )
        .unwrap();
        let validated = validate_upstream_response(
            response(StatusCode::OK, &[("content-type", "video/mp4")]),
            &conditional,
        )
        .unwrap();
        assert_eq!(validated.status, 200);
    }

    #[test]
    fn rejects_unsatisfied_range_without_a_range_request() {
        let request = RemoteStreamRequest::new(RemoteStreamMethod::Get, None, None).unwrap();
        let error = validate_upstream_response(
            response(
                StatusCode::RANGE_NOT_SATISFIABLE,
                &[("content-range", "bytes */100")],
            ),
            &request,
        )
        .unwrap_err();
        assert_eq!(error.kind(), RemoteStreamErrorKind::ResponseInvalid);
    }

    #[test]
    fn rejects_html_json_xml_and_hls_content_types() {
        let request = RemoteStreamRequest::new(RemoteStreamMethod::Get, None, None).unwrap();
        for content_type in [
            "text/html",
            "application/json",
            "application/xml",
            "application/vnd.apple.mpegurl",
            "audio/x-mpegurl",
        ] {
            let error = validate_upstream_response(
                response(StatusCode::OK, &[("content-type", content_type)]),
                &request,
            )
            .unwrap_err();
            assert_eq!(error.kind(), RemoteStreamErrorKind::ResponseInvalid);
        }
    }

    #[test]
    fn rejects_upstream_error_statuses_without_exposing_their_bodies() {
        let request = RemoteStreamRequest::new(RemoteStreamMethod::Get, None, None).unwrap();
        for status in [
            StatusCode::UNAUTHORIZED,
            StatusCode::FORBIDDEN,
            StatusCode::NOT_FOUND,
            StatusCode::INTERNAL_SERVER_ERROR,
        ] {
            let error = validate_upstream_response(response(status, &[]), &request).unwrap_err();
            assert_eq!(error.kind(), RemoteStreamErrorKind::SourceUnavailable);
        }
    }

    #[test]
    fn forwards_only_the_explicit_safe_response_header_set() {
        let request = RemoteStreamRequest::new(RemoteStreamMethod::Get, None, None).unwrap();
        let validated = validate_upstream_response(
            response(
                StatusCode::OK,
                &[
                    ("content-type", "application/octet-stream"),
                    ("content-length", "10"),
                    ("accept-ranges", "bytes"),
                    ("etag", "\"abc\""),
                    ("last-modified", "Sun, 06 Nov 1994 08:49:37 GMT"),
                    ("set-cookie", "session=secret"),
                    ("location", "https://secret.example/file"),
                    ("server", "private-upstream"),
                ],
            ),
            &request,
        )
        .unwrap();

        let names = validated
            .headers
            .iter()
            .map(|header| header.name().as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            vec![
                "content-type",
                "content-length",
                "accept-ranges",
                "etag",
                "last-modified"
            ]
        );
    }

    #[test]
    fn rejects_encoded_bodies_that_cannot_be_described_by_the_safe_header_set() {
        let request = RemoteStreamRequest::new(RemoteStreamMethod::Get, None, None).unwrap();
        let error = validate_upstream_response(
            response(
                StatusCode::OK,
                &[("content-type", "video/mp4"), ("content-encoding", "gzip")],
            ),
            &request,
        )
        .unwrap_err();
        assert_eq!(error.kind(), RemoteStreamErrorKind::ResponseInvalid);
    }

    #[test]
    fn normalizes_head_get_probe_metadata_without_forwarding_probe_body_length() {
        let validated = validate_head_probe_response(response(
            StatusCode::PARTIAL_CONTENT,
            &[
                ("content-type", "video/mp4"),
                ("content-range", "bytes 0-0/500"),
                ("content-length", "1"),
            ],
        ))
        .unwrap();
        assert_eq!(validated.status, 200);
        assert!(validated.response.is_none());
        assert!(validated.headers.iter().any(|header| {
            header.name() == RemoteResponseHeaderName::ContentLength && header.value() == b"500"
        }));
    }

    #[test]
    fn accepts_only_well_formed_416_content_range() {
        let request =
            RemoteStreamRequest::new(RemoteStreamMethod::Get, Some("bytes=100-"), None).unwrap();
        let validated = validate_upstream_response(
            response(
                StatusCode::RANGE_NOT_SATISFIABLE,
                &[("content-range", "bytes */90"), ("set-cookie", "secret=1")],
            ),
            &request,
        )
        .unwrap();
        assert_eq!(validated.status, 416);
        assert_eq!(validated.headers.len(), 1);
        assert_eq!(
            validated.headers[0].name(),
            RemoteResponseHeaderName::ContentRange
        );
    }
}
