use fast_socks5::server::Socks5ServerProtocol;
use fast_socks5::{ReplyError, Socks5Command, SocksError};
use tokio::sync::Mutex;
use tracing::Instrument;
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream, ToSocketAddrs, UdpSocket};
use crate::dispatcher::{Dispatcher, DispatcherResponse};
use crate::{TargetAddr, AlphaRayError};
use crate::inbound::InboundRequest;
use super::Inbound;

pub struct Socks5Inbound {
    listener: TcpListener,
}

impl Socks5Inbound {
    pub async fn bind(addr: SocketAddr) -> crate::Result<Self> {
        let listener = TcpListener::bind(addr).await?;

        Ok(Self {
            listener
        })
    }
}

async fn handle_socks5_stream(stream: TcpStream, dispatcher: Arc<dyn Dispatcher>) -> crate::Result<()> {
    let peer_addr = stream.peer_addr().unwrap();

    let (protocol, cmd, target_addr) = Socks5ServerProtocol::accept_no_auth(stream)
        .await
        .map_err(Into::<SocksError>::into)?
        .read_command()
        .await
        .map_err(Into::<SocksError>::into)?;

    match cmd {
        Socks5Command::TCPConnect => {
            tracing::info!("Received `TCPConnect` command from `{}` to `{}`", peer_addr, target_addr);

            let src;
            let dst;

            let req = InboundRequest::Connect {
                target: TargetAddr::from(target_addr)
            };
            match dispatcher.dispatch(req).await {
                Ok(res) => {
                    tracing::info!("Connected, request to tunnel");
                    if let DispatcherResponse::Connected { stream } = res {
                        src = Box::new(protocol
                            .reply_success(SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 0))
                            .await
                            .map_err(Into::<SocksError>::into)?);
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

                    protocol
                        .reply_error(&ReplyError::HostUnreachable)
                        .await
                        .map_err(Into::<SocksError>::into)?;
                }
            }
        },
        _ => {
            protocol.reply_error(&ReplyError::CommandNotSupported).await
                .map_err(Into::<SocksError>::into)?;

            return Err(AlphaRayError::Socks(ReplyError::CommandNotSupported.into()))
        }
    }

    Ok(())
}

#[async_trait::async_trait]
impl Inbound for Socks5Inbound {
    async fn incoming(&self, dispatcher: Arc<dyn Dispatcher>) -> crate::Result<()> {
        let span = self.span();
        async move {
            loop {
                let (stream, peer_addr) = self.listener.accept().await?;

                let stream_span = tracing::info_span!("stream", peer = %peer_addr);

                let dispatcher = dispatcher.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_socks5_stream(stream, dispatcher).await {
                        tracing::error!("Stream handling error: {}", e);
                    }
                }.instrument(stream_span));
            }
        }
            .instrument(span)
            .await
    }

    fn name(&self) -> &str {
        "Socks5Inbound"
    }
}
