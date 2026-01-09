pub mod none;

use crate::{AsyncStream, TargetAddr};

#[async_trait::async_trait]
pub trait StreamInboundNetwork: Send + Sync {
    async fn accept(&self, stream: Box<dyn AsyncStream>) -> crate::Result<Box<dyn AsyncStream>>;

    fn name(&self) -> &str;
}

#[async_trait::async_trait]
pub trait StreamOutboundNetwork: Send + Sync {
    async fn upgrade(&self, stream: Box<dyn AsyncStream>) -> crate::Result<Box<dyn AsyncStream>>;

    fn name(&self) -> &str;
}
