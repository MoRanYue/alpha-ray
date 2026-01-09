pub mod tcp;
pub mod socks;
pub mod http;
pub mod vless;

use crate::{AsyncStream, TargetAddr};

#[async_trait::async_trait]
pub trait Outbound: Send + Sync {
    async fn connect(&self, target: TargetAddr) -> crate::Result<Box<dyn AsyncStream>>;

    fn name(&self) -> &str;
}

