use super::{
    errors::{source_unavailable, target_forbidden, RemoteStreamError},
    resolver::{DnsResolutionError, RemoteDnsResolver},
};
use reqwest::Url;
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fmt,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
};

#[derive(Clone, PartialEq, Eq, Hash)]
struct AllowedHostPort {
    host: String,
    port: u16,
}

#[derive(Clone, Default)]
pub(crate) struct RemoteTargetPolicy {
    allowed_private_targets: HashSet<AllowedHostPort>,
    #[cfg(test)]
    allow_permanently_forbidden_for_tests: bool,
}

#[derive(Clone)]
pub(crate) struct ValidatedRemoteTarget {
    pub(crate) url: Url,
    pub(crate) hostname: String,
    pub(crate) addresses: Vec<SocketAddr>,
    pub(crate) diagnostics: RemoteTargetDiagnostics,
}

#[derive(Clone, PartialEq, Eq)]
pub struct RemoteTargetDiagnostics {
    scheme: &'static str,
    host_fingerprint: String,
    port: u16,
    reference_hash_prefix: String,
}

impl RemoteTargetDiagnostics {
    pub fn scheme(&self) -> &'static str {
        self.scheme
    }

    pub fn host_fingerprint(&self) -> &str {
        &self.host_fingerprint
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn reference_hash_prefix(&self) -> &str {
        &self.reference_hash_prefix
    }
}

impl fmt::Debug for RemoteTargetDiagnostics {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RemoteTargetDiagnostics")
            .field("scheme", &self.scheme)
            .field("host_fingerprint", &self.host_fingerprint)
            .field("port", &self.port)
            .field("reference_hash_prefix", &self.reference_hash_prefix)
            .finish()
    }
}

impl fmt::Debug for ValidatedRemoteTarget {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ValidatedRemoteTarget")
            .field("diagnostics", &self.diagnostics)
            .field("address_count", &self.addresses.len())
            .finish()
    }
}

impl RemoteTargetPolicy {
    pub(crate) fn from_allowed_hosts(value: Option<&str>) -> Result<Self, &'static str> {
        let mut allowed_private_targets = HashSet::new();
        let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
            return Ok(Self::default());
        };

        for entry in value.split(',') {
            let entry = entry.trim();
            if entry.is_empty() || entry.contains('*') || entry.contains('/') {
                return Err("MOVA_STRM_ALLOWED_HOSTS entries must be exact host:port values");
            }
            let (host, port) = parse_allowed_host_port(entry)?;
            allowed_private_targets.insert(AllowedHostPort { host, port });
        }

        Ok(Self {
            allowed_private_targets,
            #[cfg(test)]
            allow_permanently_forbidden_for_tests: false,
        })
    }

    pub(crate) async fn validate(
        &self,
        url: Url,
        reference_hash: &str,
        resolver: &dyn RemoteDnsResolver,
    ) -> Result<ValidatedRemoteTarget, RemoteStreamError> {
        if url.as_str().len() > mova_scan::MAX_STRM_URL_BYTES {
            return Err(target_forbidden());
        }
        let scheme = match url.scheme() {
            "http" => "http",
            "https" => "https",
            _ => return Err(target_forbidden()),
        };
        if !url.username().is_empty() || url.password().is_some() || url.fragment().is_some() {
            return Err(target_forbidden());
        }

        let port = url.port_or_known_default().ok_or_else(target_forbidden)?;
        if port == 0 {
            return Err(target_forbidden());
        }

        url.host().ok_or_else(target_forbidden)?;
        let connection_hostname =
            normalize_connection_host(url.host_str().ok_or_else(target_forbidden)?);
        let policy_hostname = normalize_host(&connection_hostname);
        if policy_hostname == "localhost"
            || policy_hostname.ends_with(".localhost")
            || is_known_metadata_hostname(&policy_hostname)
        {
            return Err(target_forbidden());
        }
        let allow_private = self.allowed_private_targets.contains(&AllowedHostPort {
            host: policy_hostname.clone(),
            port,
        });

        let ip_literal = connection_hostname
            .strip_prefix('[')
            .and_then(|value| value.strip_suffix(']'))
            .unwrap_or(&connection_hostname)
            .parse::<IpAddr>()
            .ok();
        let addresses =
            match ip_literal {
                Some(address) => vec![SocketAddr::new(address, port)],
                None => resolver
                    .resolve(&connection_hostname, port)
                    .await
                    .map_err(|error| match error {
                        DnsResolutionError::TimedOut => super::errors::source_timeout(),
                        DnsResolutionError::Failed => source_unavailable(),
                    })?,
            };
        if addresses.is_empty() {
            return Err(source_unavailable());
        }

        for address in &addresses {
            if address.port() != port {
                return Err(target_forbidden());
            }
            match classify_ip(address.ip()) {
                IpPolicyClass::Public => {}
                IpPolicyClass::Private if allow_private => {}
                IpPolicyClass::Private | IpPolicyClass::PermanentlyForbidden => {
                    #[cfg(test)]
                    if self.allow_permanently_forbidden_for_tests {
                        continue;
                    }
                    return Err(target_forbidden());
                }
            }
        }

        let host_fingerprint = format!("{:x}", Sha256::digest(policy_hostname.as_bytes()))
            .chars()
            .take(12)
            .collect();
        Ok(ValidatedRemoteTarget {
            url,
            hostname: connection_hostname,
            addresses,
            diagnostics: RemoteTargetDiagnostics {
                scheme,
                host_fingerprint,
                port,
                reference_hash_prefix: reference_hash.chars().take(8).collect(),
            },
        })
    }

    #[cfg(test)]
    pub(crate) fn unsafe_allow_loopback_for_mock_upstream() -> Self {
        Self {
            allowed_private_targets: HashSet::new(),
            allow_permanently_forbidden_for_tests: true,
        }
    }
}

fn parse_allowed_host_port(value: &str) -> Result<(String, u16), &'static str> {
    if let Ok(address) = value.parse::<SocketAddr>() {
        if address.port() == 0 {
            return Err("MOVA_STRM_ALLOWED_HOSTS ports must be greater than zero");
        }
        if classify_ip(address.ip()) == IpPolicyClass::PermanentlyForbidden {
            return Err("MOVA_STRM_ALLOWED_HOSTS cannot allow a permanently forbidden address");
        }
        return Ok((normalize_host(&address.ip().to_string()), address.port()));
    }

    let (host, port) = value
        .rsplit_once(':')
        .ok_or("MOVA_STRM_ALLOWED_HOSTS entries must include an explicit port")?;
    if !is_valid_allowlist_hostname(host) {
        return Err("MOVA_STRM_ALLOWED_HOSTS contains an invalid hostname");
    }
    let port = port
        .parse::<u16>()
        .ok()
        .filter(|port| *port > 0)
        .ok_or("MOVA_STRM_ALLOWED_HOSTS contains an invalid port")?;
    let host = normalize_host(host);
    if host == "localhost" || host.ends_with(".localhost") || is_known_metadata_hostname(&host) {
        return Err("MOVA_STRM_ALLOWED_HOSTS cannot allow a permanently forbidden hostname");
    }
    Ok((host, port))
}

fn is_valid_allowlist_hostname(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= 253
        && !host.contains(':')
        && !host.starts_with('.')
        && !host.ends_with('.')
        && host.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
                && label
                    .as_bytes()
                    .first()
                    .is_some_and(u8::is_ascii_alphanumeric)
                && label
                    .as_bytes()
                    .last()
                    .is_some_and(u8::is_ascii_alphanumeric)
        })
}

fn normalize_host(host: &str) -> String {
    normalize_connection_host(host)
        .trim_end_matches('.')
        .to_string()
}

fn normalize_connection_host(host: &str) -> String {
    host.strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .unwrap_or(host)
        .to_ascii_lowercase()
}

fn is_known_metadata_hostname(host: &str) -> bool {
    matches!(
        host,
        "metadata"
            | "metadata.google.internal"
            | "metadata.goog"
            | "instance-data"
            | "instance-data.ec2.internal"
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IpPolicyClass {
    Public,
    Private,
    PermanentlyForbidden,
}

fn classify_ip(address: IpAddr) -> IpPolicyClass {
    match address {
        IpAddr::V4(address) => classify_ipv4(address),
        IpAddr::V6(address) => classify_ipv6(address),
    }
}

fn classify_ipv4(address: Ipv4Addr) -> IpPolicyClass {
    let [a, b, c, d] = address.octets();

    if a == 127
        || (a == 169 && b == 254)
        || a >= 224
        || address.is_unspecified()
        || address == Ipv4Addr::new(100, 100, 100, 200)
        || address == Ipv4Addr::new(168, 63, 129, 16)
    {
        return IpPolicyClass::PermanentlyForbidden;
    }
    if a == 10
        || (a == 172 && (16..=31).contains(&b))
        || (a == 192 && b == 168)
        || (a == 100 && (64..=127).contains(&b))
    {
        return IpPolicyClass::Private;
    }

    let reserved = a == 0
        || (a == 192 && b == 0 && c == 0)
        || (a == 192 && b == 0 && c == 2)
        || (a == 192 && b == 88 && c == 99)
        || (a == 198 && (b == 18 || b == 19))
        || (a == 198 && b == 51 && c == 100)
        || (a == 203 && b == 0 && c == 113)
        || (a == 255 && b == 255 && c == 255 && d == 255);
    if reserved {
        IpPolicyClass::PermanentlyForbidden
    } else {
        IpPolicyClass::Public
    }
}

fn classify_ipv6(address: Ipv6Addr) -> IpPolicyClass {
    if let Some(mapped) = address.to_ipv4_mapped() {
        return classify_ipv4(mapped);
    }
    if address.is_unspecified()
        || address.is_loopback()
        || address.is_unicast_link_local()
        || address.is_multicast()
        || address
            == "fd00:ec2::254"
                .parse::<Ipv6Addr>()
                .expect("valid metadata IP")
        || address
            == "fd20:ce::254"
                .parse::<Ipv6Addr>()
                .expect("valid metadata IP")
    {
        return IpPolicyClass::PermanentlyForbidden;
    }
    if address.is_unique_local() {
        return IpPolicyClass::Private;
    }

    let segments = address.segments();
    let is_global_unicast = (segments[0] & 0xe000) == 0x2000;
    let documentation = segments[0] == 0x2001 && segments[1] == 0x0db8;
    let ietf_special = segments[0] == 0x2001 && segments[1] < 0x0200;
    let six_to_four = segments[0] == 0x2002;
    if !is_global_unicast || documentation || ietf_special || six_to_four {
        IpPolicyClass::PermanentlyForbidden
    } else {
        IpPolicyClass::Public
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    struct StaticResolver(Vec<SocketAddr>);

    #[async_trait]
    impl RemoteDnsResolver for StaticResolver {
        async fn resolve(
            &self,
            _hostname: &str,
            _port: u16,
        ) -> Result<Vec<SocketAddr>, DnsResolutionError> {
            Ok(self.0.clone())
        }
    }

    fn socket(ip: &str, port: u16) -> SocketAddr {
        SocketAddr::new(ip.parse().unwrap(), port)
    }

    #[tokio::test]
    async fn accepts_public_targets_and_rejects_mixed_dns_answers() {
        let policy = RemoteTargetPolicy::default();
        let public = StaticResolver(vec![socket("93.184.216.34", 443)]);
        policy
            .validate(
                Url::parse("https://media.example/video.mp4?secret=hidden").unwrap(),
                "0123456789abcdef",
                &public,
            )
            .await
            .unwrap();

        let mixed = StaticResolver(vec![
            socket("93.184.216.34", 443),
            socket("192.168.1.20", 443),
        ]);
        let error = policy
            .validate(
                Url::parse("https://media.example/video.mp4").unwrap(),
                "0123456789abcdef",
                &mixed,
            )
            .await
            .unwrap_err();
        assert_eq!(error.code(), "strm_target_forbidden");
    }

    #[tokio::test]
    async fn private_allowlist_is_exact_by_host_and_port() {
        let policy = RemoteTargetPolicy::from_allowed_hosts(Some("media.home:443")).unwrap();
        let private = StaticResolver(vec![socket("192.168.1.20", 443)]);
        policy
            .validate(
                Url::parse("https://media.home/video.mp4").unwrap(),
                "hash",
                &private,
            )
            .await
            .unwrap();

        for url in [
            "https://other.home/video.mp4",
            "http://media.home:80/video.mp4",
        ] {
            assert_eq!(
                policy
                    .validate(Url::parse(url).unwrap(), "hash", &private)
                    .await
                    .unwrap_err()
                    .kind(),
                super::super::RemoteStreamErrorKind::TargetForbidden
            );
        }
    }

    #[tokio::test]
    async fn allowlist_never_opens_loopback_link_local_multicast_or_metadata() {
        for (host, ip) in [
            ("loop.home", "127.0.0.1"),
            ("link.home", "169.254.10.4"),
            ("cast.home", "239.1.2.3"),
            ("metadata.home", "100.100.100.200"),
            ("azure-wire.home", "168.63.129.16"),
            ("google-metadata.home", "fd20:ce::254"),
            ("mapped.home", "::ffff:127.0.0.1"),
        ] {
            let policy =
                RemoteTargetPolicy::from_allowed_hosts(Some(&format!("{host}:80"))).unwrap();
            let resolver = StaticResolver(vec![socket(ip, 80)]);
            let error = policy
                .validate(
                    Url::parse(&format!("http://{host}/video.mp4")).unwrap(),
                    "hash",
                    &resolver,
                )
                .await
                .unwrap_err();
            assert_eq!(error.code(), "strm_target_forbidden");
        }

        assert!(
            RemoteTargetPolicy::from_allowed_hosts(Some("metadata.google.internal:80")).is_err()
        );
    }

    #[tokio::test]
    async fn direct_ip_literals_follow_the_same_ipv4_ipv6_and_mapped_policy() {
        let policy = RemoteTargetPolicy::default();
        let unused_resolver = StaticResolver(Vec::new());
        for url in [
            "http://127.0.0.1/video",
            "http://10.0.0.1/video",
            "http://169.254.169.254/latest/meta-data",
            "http://168.63.129.16/machine",
            "http://[::1]/video",
            "http://[fd00::1]/video",
            "http://[fd20:ce::254]/computeMetadata/v1/",
            "http://[::ffff:127.0.0.1]/video",
            "http://[ff02::1]/video",
        ] {
            let error = policy
                .validate(Url::parse(url).unwrap(), "hash", &unused_resolver)
                .await
                .unwrap_err();
            assert_eq!(error.code(), "strm_target_forbidden", "{url}");
        }

        policy
            .validate(
                Url::parse("https://[2606:4700:4700::1111]/video").unwrap(),
                "hash",
                &unused_resolver,
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn diagnostics_hide_hostname_query_and_reference_hash_tail() {
        let policy = RemoteTargetPolicy::default();
        let resolver = StaticResolver(vec![socket("93.184.216.34", 443)]);
        let target = policy
            .validate(
                Url::parse("https://private-name.example/video?token=do-not-log").unwrap(),
                "0123456789abcdef-secret-tail",
                &resolver,
            )
            .await
            .unwrap();
        let rendered = format!("{target:?}");

        assert!(!rendered.contains("private-name.example"));
        assert!(!rendered.contains("do-not-log"));
        assert!(!rendered.contains("secret-tail"));
        assert!(rendered.contains("01234567"));
    }

    #[test]
    fn allowlist_parser_rejects_wildcards_cidr_missing_ports_and_localhost() {
        for value in [
            "*.home:443",
            "10.0.0.0/8",
            "media.home",
            "localhost:80",
            "127.0.0.1:80",
            "bad@host:80",
        ] {
            assert!(RemoteTargetPolicy::from_allowed_hosts(Some(value)).is_err());
        }
    }
}
