use std::net::SocketAddr;
use std::sync::Arc;
use uuid::Uuid;
use crate::transport::tcp::TcpOutboundTransport;
use crate::security::none::NoneSecurity;
use crate::network::none::NoneNetwork;
use crate::pipeline::StreamOutboundPipeline;
use crate::{AsyncStream, TargetAddr};
use crate::protocol::vless::{VlessCommand, VlessRequest, VlessStream, VlessVersion};
use super::Outbound;

pub struct VlessOutbound {
    inner: Arc<dyn Outbound>,
    server_addr: SocketAddr,
    uuid: Uuid,
    name: String
}

impl VlessOutbound {
    pub fn new(inner: Arc<dyn Outbound>, server_addr: SocketAddr, uuid: Uuid) -> Self {
        Self {
            name: format!("VlessOutbound<{}>", inner.name()),
            inner,
            server_addr,
            uuid
        }
    }

    #[cfg(debug_assertions)]
    pub fn new_with_tcp(server_addr: SocketAddr, uuid: Uuid) -> Self {
        let transport = Arc::new(TcpOutboundTransport::new());
        let security = Arc::new(NoneSecurity);
        let network = Arc::new(NoneNetwork);
        
        let pipeline = StreamOutboundPipeline::new(transport, security, network);
            
        Self::new(Arc::new(pipeline), server_addr, uuid)
    }
}

#[async_trait::async_trait]
impl Outbound for VlessOutbound {
    async fn connect(&self, target: TargetAddr) -> crate::Result<Box<dyn AsyncStream>> {
        let server_target = TargetAddr::SocketAddr(self.server_addr);
        let stream = self.inner.connect(server_target).await?;

        let req = VlessRequest::new(
            VlessVersion::Zero,
            self.uuid,
            VlessCommand::Tcp,
            target
        );
        let mut stream = VlessStream::new(stream);
        stream.connect(req).await?;
        
        Ok(Box::new(stream))
    }

    fn name(&self) -> &str {
        &self.name
    }
}
