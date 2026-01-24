use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};
use tracing::Instrument;
use crate::error::AlphaRayError;
use crate::{AsyncStream, TargetAddr};
use super::{StreamOutboundTransport, StreamInboundTransport};

pub struct TcpOutboundTransport {
    timeout: Duration,
}

impl TcpOutboundTransport {
    pub fn new() -> Self {
        Self {
            timeout: Duration::from_secs(30)
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

#[async_trait::async_trait]
impl StreamOutboundTransport for TcpOutboundTransport {
    async fn connect(&self, target: TargetAddr) -> crate::Result<Box<dyn AsyncStream>> {
        let span = self.span();
        async move {
            tracing::info!("Connecting to `{}`", target);
            
            let addrs = match target {
                TargetAddr::SocketAddr(a) => vec![a],
                TargetAddr::Domain(h, p) => {
                    tokio::net::lookup_host((h.as_str(), p)).await?.collect()
                }
            };

            for addr in addrs {
                match tokio::time::timeout(self.timeout, TcpStream::connect(addr)).await {
                    Ok(Ok(s)) => {
                        s.set_nodelay(true).ok();

                        return Ok(Box::new(s) as Box<dyn AsyncStream>);
                    }
                    Ok(Err(e)) => {
                        tracing::error!("Failed to connect to `{}`: {}", addr, e);
                    },
                    Err(_) => {
                        tracing::error!("Connection to `{}` has timed out", addr)
                    }
                }
            }

            Err(AlphaRayError::Connection)
        }
            .instrument(span)
            .await
    }

    fn name(&self) -> &str {
        "TcpOutboundTransport"
    }
}

pub struct TcpInboundTransport {
    listener: TcpListener
}

impl TcpInboundTransport {
    pub async fn bind(addr: SocketAddr) -> crate::Result<Self> {
        Ok(Self {
            listener: TcpListener::bind(addr).await?
        })
    }
}

#[async_trait::async_trait]
impl StreamInboundTransport for TcpInboundTransport {
    async fn accept(&self) -> crate::Result<(Box<dyn AsyncStream>, SocketAddr)> {
        let span = self.span();
        async move {
            let (s, p) = self.listener.accept().await?;
            tracing::debug!("Accepted connection from `{}`", p);

            Ok((Box::new(s) as Box<dyn AsyncStream>, p))
        }
            .instrument(span)
            .await
    }

    fn name(&self) -> &str {
        "TcpInboundTransport"
    }
}
