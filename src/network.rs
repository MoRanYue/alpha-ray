pub mod none;

use crate::{AsyncStream, TargetAddr};
use tracing::Span;

#[async_trait::async_trait]
pub trait StreamInboundNetwork: Send + Sync {
    async fn accept(&self, stream: Box<dyn AsyncStream>) -> crate::Result<Box<dyn AsyncStream>>;

    fn name(&self) -> &str;

    fn span(&self) -> Span {
        tracing::info_span!("inbound_network", name = self.name())
    }
}

#[async_trait::async_trait]
pub trait StreamOutboundNetwork: Send + Sync {
    async fn upgrade(&self, stream: Box<dyn AsyncStream>) -> crate::Result<Box<dyn AsyncStream>>;

    fn name(&self) -> &str;

    fn span(&self) -> Span {
        tracing::info_span!("outbound_network", name = self.name())
    }
}
