pub mod socks;
pub mod http;
pub mod vless;

use crate::{AsyncStream, TargetAddr};
use tracing::Span;

#[async_trait::async_trait]
pub trait Outbound: Send + Sync {
    async fn connect(&self, target: TargetAddr) -> crate::Result<Box<dyn AsyncStream>>;

    fn name(&self) -> &str;

    fn span(&self) -> Span {
        tracing::info_span!("outbound", name = self.name())
    }
}

