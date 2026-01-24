use std::sync::Arc;
use tracing::Instrument;
use crate::dispatcher::DispatcherResponse;
use crate::inbound::InboundRequest;
use crate::outbound::Outbound;
use super::Dispatcher;

pub struct SimpleDispatcher {
    outbound: Arc<dyn Outbound>,
    name: Arc<str>
}

impl SimpleDispatcher {
    pub fn new(outbound: Arc<dyn Outbound>) -> Self {
        Self {
            name: Arc::from(format!("SimpleDispatcher<{}>", outbound.name()).into_boxed_str()),
            outbound
        }
    }
}

#[async_trait::async_trait]
impl Dispatcher for SimpleDispatcher {
    async fn dispatch(&self, req: InboundRequest) -> crate::Result<DispatcherResponse> {
        let span = self.span();
        async move {
            tracing::info!("Dispatching request");

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
            .instrument(span)
            .await
    }

    fn name(&self) -> &str {
        &self.name
    }
}
