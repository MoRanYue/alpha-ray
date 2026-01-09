use crate::{AsyncStream, TargetAddr};
use super::{StreamInboundNetwork, StreamOutboundNetwork};

pub struct NoneNetwork;

#[async_trait::async_trait]
impl StreamInboundNetwork for NoneNetwork {
    async fn accept(&self, stream: Box<dyn AsyncStream>) -> crate::Result<Box<dyn AsyncStream>> {
        Ok(stream)
    }

    fn name(&self) -> &str {
        "NoneNetwork"
    }
}

#[async_trait::async_trait]
impl StreamOutboundNetwork for NoneNetwork {
    async fn upgrade(&self, stream: Box<dyn AsyncStream>) -> crate::Result<Box<dyn AsyncStream>> {
        Ok(stream)
    }

    fn name(&self) -> &str {
        "NoneNetwork"
    }
}
