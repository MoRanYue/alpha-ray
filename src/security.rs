pub mod none;
pub mod tls;
pub mod vision_tls;

use crate::{AsyncStream, TargetAddr};
use tracing::Span;

#[async_trait::async_trait]
pub trait StreamInboundSecurity: Send + Sync {
    async fn accept(&self, stream: Box<dyn AsyncStream>) -> crate::Result<Box<dyn AsyncStream>>;

    fn name(&self) -> &str;

    fn span(&self) -> Span {
        tracing::info_span!("inbound_security", name = self.name())
    }
}

#[async_trait::async_trait]
pub trait StreamOutboundSecurity: Send + Sync {
    async fn wrap(&self, stream: Box<dyn AsyncStream>) -> crate::Result<Box<dyn AsyncStream>>;

    fn name(&self) -> &str;

    fn span(&self) -> Span {
        tracing::info_span!("outbound_security", name = self.name())
    }
}
