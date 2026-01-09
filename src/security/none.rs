use crate::{AsyncStream, TargetAddr};
use super::{StreamInboundSecurity, StreamOutboundSecurity};

pub struct NoneSecurity;

#[async_trait::async_trait]
impl StreamInboundSecurity for NoneSecurity {
    async fn accept(&self, stream: Box<dyn AsyncStream>) -> crate::Result<Box<dyn AsyncStream>> {
        Ok(stream)
    }

    fn name(&self) -> &str {
        "NoneSecurity"
    }
}

#[async_trait::async_trait]
impl StreamOutboundSecurity for NoneSecurity {
    async fn wrap(&self, stream: Box<dyn AsyncStream>) -> crate::Result<Box<dyn AsyncStream>> {
        Ok(stream)
    }

    fn name(&self) -> &str {
        "NoneSecurity"
    }
}
