pub mod simple;

use std::fmt::{Display, Formatter};
use std::net::{SocketAddr, ToSocketAddrs};
use std::str::FromStr;
use tokio::io::{AsyncRead, AsyncWrite};
use crate::{error::AlphaRayError, inbound::InboundRequest, AsyncStream};

#[async_trait::async_trait]
pub trait Dispatcher: Send + Sync {
    async fn dispatch(&self, req: InboundRequest) -> crate::Result<DispatcherResponse>;
}

pub enum DispatcherResponse {
    Connected {
        stream: Box<dyn AsyncStream>
    },
    Tunneling
}
