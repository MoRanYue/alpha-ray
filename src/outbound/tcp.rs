use tokio::net::TcpStream;
use crate::{AsyncStream, TargetAddr};
use crate::error::AlphaRayError;
use super::Outbound;

pub struct TcpOutbound {}

impl TcpOutbound {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait::async_trait]
impl Outbound for TcpOutbound {
    async fn connect(&self, target: TargetAddr) -> crate::Result<Box<dyn AsyncStream>> {
        match target {
            TargetAddr::SocketAddr(addr) => {
                let s = TcpStream::connect(addr).await?;

                tracing::info!("Attempting to connect to `{}`", addr);

                Ok(Box::new(s))
            },
            TargetAddr::Domain(name, port) => {
                let addrs = tokio::net::lookup_host((name.as_str(), port)).await?;
                let addr = addrs
                    .into_iter()
                    .next()
                    .ok_or(AlphaRayError::Other(anyhow::anyhow!("Empty DNS response")))?;
                let s = TcpStream::connect(addr).await?;

                tracing::info!("Attempting to connect to `{}:{} ({})`", name, port, addr);
                
                Ok(Box::new(s))
            }
        }
    }

    fn name(&self) -> &str {
        "TcpOutbound"
    }
}
