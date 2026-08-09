use async_trait::async_trait;
use std::{
    collections::HashSet,
    net::{SocketAddr, ToSocketAddrs},
    time::Duration,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DnsResolutionError {
    Failed,
    TimedOut,
}

#[async_trait]
pub(crate) trait RemoteDnsResolver: Send + Sync {
    async fn resolve(
        &self,
        hostname: &str,
        port: u16,
    ) -> Result<Vec<SocketAddr>, DnsResolutionError>;
}

#[derive(Debug, Clone)]
pub(crate) struct SystemDnsResolver {
    timeout: Duration,
}

impl SystemDnsResolver {
    pub(crate) fn new(timeout: Duration) -> Self {
        Self { timeout }
    }
}

#[async_trait]
impl RemoteDnsResolver for SystemDnsResolver {
    async fn resolve(
        &self,
        hostname: &str,
        port: u16,
    ) -> Result<Vec<SocketAddr>, DnsResolutionError> {
        let hostname = hostname.to_owned();
        let lookup = tokio::task::spawn_blocking(move || {
            (hostname.as_str(), port)
                .to_socket_addrs()
                .map(|addresses| {
                    let mut seen = HashSet::new();
                    addresses
                        .filter(|address| seen.insert(*address))
                        .collect::<Vec<_>>()
                })
                .map_err(|_| DnsResolutionError::Failed)
        });

        tokio::time::timeout(self.timeout, lookup)
            .await
            .map_err(|_| DnsResolutionError::TimedOut)?
            .map_err(|_| DnsResolutionError::Failed)?
    }
}
