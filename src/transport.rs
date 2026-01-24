pub mod tcp;

use std::net::SocketAddr;
use crate::{AsyncStream, TargetAddr};
use tracing::Span;

#[async_trait::async_trait]
pub trait StreamInboundTransport: Send + Sync {
    async fn accept(&self) -> crate::Result<(Box<dyn AsyncStream>, SocketAddr)>;

    fn name(&self) -> &str;

    fn span(&self) -> Span {
        tracing::info_span!("inbound_transport", name = self.name())
    }
}

#[async_trait::async_trait]
pub trait StreamOutboundTransport: Send + Sync {
    async fn connect(&self, target: TargetAddr) -> crate::Result<Box<dyn AsyncStream>>;

    fn name(&self) -> &str;

    fn span(&self) -> Span {
        tracing::info_span!("outbound_transport", name = self.name())
    }
}
