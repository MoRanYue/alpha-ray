use std::ops::RangeInclusive;
use std::sync::Arc;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, ServerName};
use rustls::{ClientConfig, ClientConnection, Connection, RootCertStore, ServerConfig, ServerConnection};
use uuid::Uuid;
use crate::protocol::vision_tls::VisionTlsStream;
use crate::{AsyncStream, TargetAddr};
use super::{StreamOutboundSecurity, StreamInboundSecurity};

pub struct VisionTlsOutboundSecurity {
    config: Arc<ClientConfig>,
    server_name: ServerName<'static>,
    id: Uuid,
    padding_len_range: RangeInclusive<u16>,
    long_padding_threshold: u16,
    long_padding_base_len: u16,
    long_padding_len_range: RangeInclusive<u16>
}

impl VisionTlsOutboundSecurity {
    pub fn new(
        config: Arc<ClientConfig>,
        server_name: ServerName<'static>,
        id: Uuid,
        padding_len_range: RangeInclusive<u16>,
        long_padding_threshold: u16,
        long_padding_base_len: u16,
        long_padding_len_range: RangeInclusive<u16>
    ) -> Self {
        Self {
            config,
            server_name,
            id,
            padding_len_range,
            long_padding_threshold,
            long_padding_base_len,
            long_padding_len_range
        }
    }

    pub fn new_with_mozilla_and_default_padding(server_name: ServerName<'static>, id: Uuid) -> Self {
        let mut root_store = RootCertStore::from_iter(
            webpki_roots::TLS_SERVER_ROOTS.iter().cloned()
        );
        root_store.add(CertificateDer::from_pem_file("D:/RUST/alpha-ray/certs/root.crt").unwrap());
        let config = Arc::new(
            rustls::ClientConfig::builder()
                .with_root_certificates(root_store)
                .with_no_client_auth()
        );

        Self::new(config, server_name, id, 0..=256, 900, 500, 0..=500)
    }
}

#[async_trait::async_trait]
impl StreamOutboundSecurity for VisionTlsOutboundSecurity {
    async fn wrap(&self, stream: Box<dyn AsyncStream>) -> crate::Result<Box<dyn AsyncStream>> {
        let conn = ClientConnection::new(self.config.clone(), self.server_name.clone())?;
        let s = Box::new(VisionTlsStream::new(
            stream,
            Connection::Client(conn),
            &[self.id],
            self.padding_len_range.clone(),
            self.long_padding_threshold,
            self.long_padding_base_len,
            self.long_padding_len_range.clone()
        ));

        Ok(s)
    }

    fn name(&self) -> &str {
        "VisionTlsOutboundSecurity"
    }
}

pub struct VisionTlsInboundSecurity {
    config: Arc<ServerConfig>,
    ids: Vec<Uuid>,
    padding_len_range: RangeInclusive<u16>,
    long_padding_threshold: u16,
    long_padding_base_len: u16,
    long_padding_len_range: RangeInclusive<u16>
}

impl VisionTlsInboundSecurity {
    pub fn new(
        config: Arc<ServerConfig>,
        ids: Vec<Uuid>,
        padding_len_range: RangeInclusive<u16>,
        long_padding_threshold: u16,
        long_padding_base_len: u16,
        long_padding_len_range: RangeInclusive<u16>
    ) -> Self {
        Self {
            config,
            ids,
            padding_len_range,
            long_padding_threshold,
            long_padding_base_len,
            long_padding_len_range
        }
    }

    pub fn new_with_default_padding(config: Arc<ServerConfig>, ids: Vec<Uuid>) -> Self {
        Self::new(config, ids, 0..=256, 900, 500, 0..=500)
    }
}

#[async_trait::async_trait]
impl StreamInboundSecurity for VisionTlsInboundSecurity {
    async fn accept(&self, stream: Box<dyn AsyncStream>) -> crate::Result<Box<dyn AsyncStream>> {
        let conn = ServerConnection::new(self.config.clone())?;
        let s = Box::new(VisionTlsStream::new(
            stream,
            Connection::Server(conn),
            &self.ids,
            self.padding_len_range.clone(),
            self.long_padding_threshold,
            self.long_padding_base_len,
            self.long_padding_len_range.clone()
        ));

        Ok(s)
    }

    fn name(&self) -> &str {
        "VisionTlsInboundSecurity"
    }
}
