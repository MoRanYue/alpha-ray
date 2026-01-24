pub mod socks;
pub mod http;
pub mod vless;

use std::sync::Arc;
use tracing::{Level, Span};

use crate::dispatcher::Dispatcher;
use crate::{AsyncStream, TargetAddr};

#[async_trait::async_trait]
pub trait Inbound: Send + Sync {
    async fn incoming(&self, dispatcher: Arc<dyn Dispatcher>) -> crate::Result<()>;

    fn name(&self) -> &str;

    fn span(&self) -> Span {
        tracing::info_span!("inbound", name = self.name())
    }
}

pub enum InboundRequest {
    Connect {
        target: TargetAddr
    },
    Tunnel {
        src: Box<dyn AsyncStream>,
        dst: Box<dyn AsyncStream>
    }
}
