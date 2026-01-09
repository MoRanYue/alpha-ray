pub mod tcp;

use crate::{AsyncStream, TargetAddr};

#[async_trait::async_trait]
pub trait StreamInboundTransport: Send + Sync {
    async fn accept(&self) -> crate::Result<Box<dyn AsyncStream>>;

    fn name(&self) -> &str;
}

#[async_trait::async_trait]
pub trait StreamOutboundTransport: Send + Sync {
    async fn connect(&self, target: TargetAddr) -> crate::Result<Box<dyn AsyncStream>>;

    fn name(&self) -> &str;
}
