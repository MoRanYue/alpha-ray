use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use uuid::Uuid;
use crate::error::{AlphaRayError, VlessError};
use crate::pipeline::StreamInboundPipeline;
use crate::{AsyncStream, TargetAddr};
use crate::dispatcher::{Dispatcher, DispatcherResponse};
use crate::inbound::InboundRequest;
use crate::protocol::vless::{VlessCommand, VlessProtocol};
use super::Inbound;

pub struct VlessInbound {
    inner: Arc<StreamInboundPipeline>,
    id: Uuid,
    name: String
}

impl VlessInbound {
    pub async fn bind(inner: Arc<StreamInboundPipeline>, id: Uuid) -> crate::Result<Self> {
        Ok(Self {
            name: format!("VlessInbound<{}>", inner.name()),
            inner,
            id
        })
    }
}

async fn handle_vless_stream(mut stream: Box<dyn AsyncStream>, id: Uuid, dispatcher: Arc<dyn Dispatcher>) -> crate::Result<()> {
    let req = VlessProtocol::accept(&mut stream).await?;
    tracing::info!("Received a request: {:?}", req);

    if req.id() != &id {
        return Err(AlphaRayError::Permission);
    }

    match req.command() {
        VlessCommand::Tcp => {
            let src = Box::new(stream);
            let dst;

            let req = InboundRequest::Connect {
                target: req.target().clone()
            };
            match dispatcher.dispatch(req).await {
                Ok(res) => {
                    tracing::info!("Connected, request to tunnel");
                    if let DispatcherResponse::Connected { stream } = res {
                        dst = Box::new(stream);

                        let req = InboundRequest::Tunnel { src, dst };
                        match dispatcher.dispatch(req).await {
                            Ok(res) => {
                                if let DispatcherResponse::Tunneling = res {}
                                else {
                                    tracing::error!("Tunneling was not started");
                                }
                            },
                            Err(e) => {
                                tracing::error!("Failed to proxy: {}", e);
                            }
                        }
                    }
                },
                Err(e) => {
                    tracing::error!("Failed to proxy: {}", e);
                }
            }
        },
        c => return Err(AlphaRayError::Vless(VlessError::InvalidCommand(c.clone() as u8)))
    }

    Ok(())
}

#[async_trait::async_trait]
impl Inbound for VlessInbound {
    async fn incoming(&self, dispatcher: Arc<dyn Dispatcher>) -> crate::Result<()> {
        loop {
            let stream = self.inner.accept().await?;

            let dispatcher = dispatcher.clone();
            let id = self.id;
            tokio::spawn(async move {
                if let Err(e) = handle_vless_stream(stream, id, dispatcher).await {
                    tracing::error!("Stream handling error: {}", e);
                }
            });
        }
    }

    fn name(&self) -> &str {
        &self.name
    }
}