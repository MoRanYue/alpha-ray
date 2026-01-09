use std::sync::Arc;
use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, Connection, RootCertStore};
use crate::protocol::tls::TlsStream;
use crate::{AsyncStream, TargetAddr};
use super::StreamOutboundSecurity;

pub struct TlsOutboundSecurity {
    config: Arc<ClientConfig>,
    server_name: ServerName<'static>
}

impl TlsOutboundSecurity {
    pub fn new(config: Arc<ClientConfig>, server_name: ServerName<'static>) -> Self {
        Self {
            config,
            server_name
        }
    }

    pub fn new_with_mozilla(server_name: ServerName<'static>) -> Self {
        let root_store = RootCertStore::from_iter(
            webpki_roots::TLS_SERVER_ROOTS.iter().cloned()
        );
        let config = Arc::new(
            rustls::ClientConfig::builder()
                .with_root_certificates(root_store)
                .with_no_client_auth()
        );

        Self::new(config, server_name)
    }
}

#[async_trait::async_trait]
impl StreamOutboundSecurity for TlsOutboundSecurity {
    async fn wrap(&self, stream: Box<dyn AsyncStream>) -> crate::Result<Box<dyn AsyncStream>> {
        let conn = ClientConnection::new(self.config.clone(), self.server_name.clone())?;
        let s = Box::new(TlsStream::new(stream, Connection::Client(conn)));

        Ok(s)
    }

    fn name(&self) -> &str {
        "TlsOutboundSecurity"
    }
}
