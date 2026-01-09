pub mod error;
pub mod dispatcher;
pub mod inbound;
pub mod outbound;
pub mod protocol;
pub mod transport;
pub mod network;
pub mod security;
pub mod pipeline;
pub mod util;

use rustls::ServerConfig;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, DnsName, PrivateKeyDer, ServerName};
use tracing_subscriber::{fmt, util::SubscriberInitExt};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use std::sync::Arc;
use uuid::Uuid;
use error::AlphaRayError;
use dispatcher::simple::SimpleDispatcher;
use inbound::Inbound;
use inbound::vless::VlessInbound;
use outbound::socks::Socks5Outbound;
use outbound::vless::VlessOutbound;
use crate::inbound::socks::Socks5Inbound;
use crate::network::none::NoneNetwork;
use crate::outbound::Outbound;
use crate::outbound::tcp::TcpOutbound;
use crate::pipeline::{StreamInboundPipeline, StreamOutboundPipeline};
use crate::security::none::NoneSecurity;
use crate::security::vision_tls::{VisionTlsInboundSecurity, VisionTlsOutboundSecurity};
use crate::transport::tcp::{TcpInboundTransport, TcpOutboundTransport};

pub use util::{Result, AsyncStream, TargetAddr};

pub async fn run() -> Result<()> {
    setup_logger();
    tracing::info!("Alpha-ray Core ({}): Try to penetrate the wall of Dystopia", "0.1.0");

    let mut handles = Vec::with_capacity(2);

    let outbound = Arc::new(TcpOutbound::new());
    
    let dispatcher = Arc::new(SimpleDispatcher::new(outbound));

    let id = Uuid::parse_str("c19c0f24-6402-4f06-8325-d555f26ddd87").unwrap();

    let config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(
            CertificateDer::pem_file_iter("D:/RUST/alpha-ray/certs/cert.crt")?
                .map(|c| c.unwrap())
                .collect(),
            PrivateKeyDer::from_pem_file("D:/RUST/alpha-ray/certs/cert.key")?
        )?;

    let pipeline = StreamInboundPipeline::new(
        Arc::new(TcpInboundTransport::bind("127.0.0.1:1082".parse().unwrap()).await?),
        Arc::new(VisionTlsInboundSecurity::new_with_default_padding(Arc::new(config), vec![id])),
        Arc::new(NoneNetwork)
    );

    let inbound = VlessInbound::bind(
        Arc::new(pipeline),
        id
    ).await?;
    tracing::info!("`{}` is listening on `127.0.0.1:1082`", inbound.name());

    handles.push(tokio::spawn(async move {
        if let Err(e) = inbound.incoming(dispatcher).await {
            tracing::error!("`{}` failed to accept: {}", inbound.name(), e);
        }
    }));

    let pipeline = StreamOutboundPipeline::new(
        Arc::new(TcpOutboundTransport::new()),
        Arc::new(VisionTlsOutboundSecurity::new_with_mozilla_and_default_padding(
            "local.caughtwind.top".try_into().unwrap(),
            id
        )),
        Arc::new(NoneNetwork)
    );

    let outbound = Arc::new(VlessOutbound::new(
        Arc::new(pipeline),
        "127.0.0.1:1082".parse().unwrap(),
        id
    ));

    let dispatcher = Arc::new(SimpleDispatcher::new(outbound));

    let inbound = Socks5Inbound::bind("127.0.0.1:1083".parse().unwrap()).await?;
    tracing::info!("`{}` is listening on `127.0.0.1:1083`", inbound.name());

    handles.push(tokio::spawn(async move {
        if let Err(e) = inbound.incoming(dispatcher).await {
            tracing::error!("`{}` failed to accept: {}", inbound.name(), e);
        }
    }));

    for h in handles {
        let _ = h.await;
    }

    Ok(())
}

fn setup_logger() {
    let timer = tracing_subscriber::fmt::time::OffsetTime::new(
        time::UtcOffset::current_local_offset().unwrap(),
        time::format_description::well_known::Rfc3339
    );

    tracing_subscriber::registry()
        .with(EnvFilter::new(
            std::env::var("ALPHA_RAY_CORE_LOG_LEVEL").unwrap_or_else(|_| "debug".into()),
        ))
        .with(
            tracing_subscriber::fmt::layer()
                .with_timer(timer)
        )
        .init();
}