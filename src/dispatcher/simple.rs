use std::sync::Arc;
use crate::dispatcher::DispatcherResponse;
use crate::inbound::InboundRequest;
use crate::outbound::Outbound;
use super::Dispatcher;

pub struct SimpleDispatcher {
    outbound: Arc<dyn Outbound>,
}

impl SimpleDispatcher {
    pub fn new(outbound: Arc<dyn Outbound>) -> Self {
        Self { outbound }
    }
}

#[async_trait::async_trait]
impl Dispatcher for SimpleDispatcher {
    async fn dispatch(&self, req: InboundRequest) -> crate::Result<DispatcherResponse> {
        tracing::info!("Dispatching request to `{}`", self.outbound.name());

        Ok(match req {
            InboundRequest::Connect { target } => {
                tracing::info!("Connecting to `{}`", target);
                let stream = self.outbound.connect(target).await?;

                DispatcherResponse::Connected { stream }
            },
            InboundRequest::Tunnel { mut src, mut dst } => {
                tracing::info!("Tunneling");
                tokio::spawn(async move {
                    match tokio::io::copy_bidirectional(&mut *src, &mut *dst).await {
                        Ok((up, down)) => {
                            tracing::debug!("Tunnel has finished, uploaded {}B, downloaded {}B", up, down);
                        },
                        Err(e) => {
                            tracing::error!("Failed to tunnel: {}", e);
                        }
                    }
                });

                DispatcherResponse::Tunneling
            }
        })
    }
}
