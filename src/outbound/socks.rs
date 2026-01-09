use std::net::SocketAddr;
use fast_socks5::client::{Config, Socks5Stream};
use crate::{AsyncStream, TargetAddr};
use crate::error::AlphaRayError;
use super::Outbound;

pub struct Socks5Outbound {
    server_addr: SocketAddr
}

impl Socks5Outbound {
    pub fn new(server_addr: SocketAddr) -> Self {
        Self { server_addr }
    }
}

#[async_trait::async_trait]
impl Outbound for Socks5Outbound {
    async fn connect(&self, target: TargetAddr) -> crate::Result<Box<dyn AsyncStream>> {
        let target_addr = match target {
            TargetAddr::SocketAddr(addr) => addr,
            TargetAddr::Domain(name, port) => {
                tokio::net::lookup_host((name.as_str(), port))
                    .await?
                    .into_iter()
                    .next()
                    .ok_or(AlphaRayError::Other(anyhow::anyhow!("Empty DNS response")))?
            }
        };

        let stream = Socks5Stream::connect(
            self.server_addr,
            target_addr.ip().to_string(),
            target_addr.port(),
            Config::default()
        ).await?;

        Ok(Box::new(stream))
    }

    fn name(&self) -> &str {
        "Socks5Outbound"
    }
}
