use std::sync::Arc;
use crate::dispatcher::Dispatcher;
use crate::inbound::Inbound;
use crate::security::none::NoneSecurity;
use crate::network::none::NoneNetwork;
use crate::{AsyncStream, TargetAddr, security};
use crate::transport::{StreamInboundTransport, StreamOutboundTransport};
use crate::security::{StreamInboundSecurity, StreamOutboundSecurity};
use crate::network::{StreamInboundNetwork, StreamOutboundNetwork};
use crate::outbound::Outbound;

pub struct StreamInboundPipeline {
    transport: Arc<dyn StreamInboundTransport>,
    security: Arc<dyn StreamInboundSecurity>,
    network: Arc<dyn StreamInboundNetwork>,
    name: String
}

impl StreamInboundPipeline {
    pub fn new(
        transport: Arc<dyn StreamInboundTransport>,
        security: Arc<dyn StreamInboundSecurity>,
        network: Arc<dyn StreamInboundNetwork>
    ) -> Self {
        Self {
            name: format!(
                "StreamInboundPipeline<{}, {}, {}>",
                transport.name(),
                security.name(),
                network.name()
            ),
            transport,
            security,
            network
        }
    }

    pub async fn accept(&self) -> crate::Result<Box<dyn AsyncStream>> {
        let mut stream = self.transport.accept().await?;
        stream = self.security.accept(stream).await?;
        stream = self.network.accept(stream).await?;
        
        Ok(stream)
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

pub struct StreamOutboundPipeline {
    transport: Arc<dyn StreamOutboundTransport>,
    security: Arc<dyn StreamOutboundSecurity>,
    network: Arc<dyn StreamOutboundNetwork>,
    name: String
}

impl StreamOutboundPipeline {
    pub fn new(
        transport: Arc<dyn StreamOutboundTransport>,
        security: Arc<dyn StreamOutboundSecurity>,
        network: Arc<dyn StreamOutboundNetwork>
    ) -> Self {
        Self {
            name: format!(
                "StreamOutboundPipeline<{}, {}, {}>",
                transport.name(),
                security.name(),
                network.name()
            ),
            transport,
            security,
            network
        }
    }
}

#[async_trait::async_trait]
impl Outbound for StreamOutboundPipeline {
    async fn connect(&self, target: TargetAddr) -> crate::Result<Box<dyn AsyncStream>> {
        let mut stream = self.transport.connect(target.clone()).await?;
        stream = self.security.wrap(stream).await?;
        stream = self.network.upgrade(stream).await?;
        
        Ok(stream)
    }

    fn name(&self) -> &str {
        &self.name
    }
}