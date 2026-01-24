pub mod simple;

use std::fmt::{Display, Formatter};
use std::net::{SocketAddr, ToSocketAddrs};
use std::str::FromStr;
use tokio::io::{AsyncRead, AsyncWrite};
use crate::{error::AlphaRayError, inbound::InboundRequest, AsyncStream};
use tracing::Span;

#[async_trait::async_trait]
pub trait Dispatcher: Send + Sync {
    async fn dispatch(&self, req: InboundRequest) -> crate::Result<DispatcherResponse>;

    fn name(&self) -> &str;

    fn span(&self) -> Span {
        tracing::info_span!("dispatcher", name = self.name())
    }
}

pub enum DispatcherResponse {
    Connected {
        stream: Box<dyn AsyncStream>
    },
    Tunneling
}
