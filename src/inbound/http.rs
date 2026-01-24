use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use std::convert::Infallible;
use hyper::body::{Incoming, Body};
use hyper::upgrade::Upgraded;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use tokio::net::{TcpListener, TcpStream};
use tracing::Instrument;
use crate::dispatcher::{Dispatcher, DispatcherResponse};
use crate::{AsyncStream, TargetAddr};
use crate::error::AlphaRayError;
use crate::inbound::InboundRequest;
use super::Inbound;

pub struct HttpInbound {
    listener: TcpListener,
}

impl HttpInbound {
    pub async fn bind(addr: SocketAddr) -> crate::Result<Self> {
        let listener = TcpListener::bind(addr).await?;

        Ok(Self {
            listener
        })
    }
}

async fn handle_upgraded(mut upgraded: Upgraded, target: TargetAddr, dispatcher: Arc<dyn Dispatcher>) -> crate::Result<()> {
    let src = Box::new(TokioIo::new(upgraded)) as Box<dyn AsyncStream>;
    let dst;

    let req = InboundRequest::Connect { target };
    match dispatcher.dispatch(req).await {
        Ok(res) => {
            tracing::info!("Connected, request to tunnel");
            if let DispatcherResponse::Connected { stream } = res {
                dst = stream;

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

    Ok(())
}

#[async_trait::async_trait]
impl Inbound for HttpInbound {
    async fn incoming(&self, dispatcher: Arc<dyn Dispatcher>) -> crate::Result<()> {
        let span = self.span();
        async move {
            loop {
                let (stream, peer_addr) = self.listener.accept().await?;

                let stream_span = tracing::info_span!("stream", peer = %peer_addr);
                let io = TokioIo::new(stream);
                let dispatcher = dispatcher.clone();
                tokio::spawn(async move {
                    if let Err(e) = hyper::server::conn::http1::Builder::new()
                        .serve_connection(io, hyper::service::service_fn(move |req| {
                            let dispatcher = dispatcher.clone();
                            async move {
                                if req.method() != Method::CONNECT {
                                    let mut res = Response::new("This server is only serving as a proxy".to_string());
                                    *res.status_mut() = StatusCode::METHOD_NOT_ALLOWED;

                                    return Ok::<_, Infallible>(res);
                                }

                                let authority = req
                                    .uri()
                                    .authority()
                                    .map(|a| a.as_str());
                                let authority = match authority {
                                    Some(a) => match a.parse::<TargetAddr>() {
                                        Ok(a) => a,
                                        Err(err) => {
                                            let mut res = Response::new(err.to_string());
                                            *res.status_mut() = StatusCode::BAD_REQUEST;

                                            return Ok(res);
                                        }
                                    },
                                    None => {
                                        let mut res = Response::new("CONNECT method needs an authority".to_string());
                                        *res.status_mut() = StatusCode::BAD_REQUEST;

                                        return Ok(res);
                                    }
                                };

                                tracing::info!("Received CONNECT from `{}` to `{}`", peer_addr, authority);

                                let res = Response::builder()
                                    .status(StatusCode::OK)
                                    .body(String::new())
                                    .unwrap();

                                tokio::spawn(async move {
                                    match hyper::upgrade::on(req).await {
                                        Ok(u) => {
                                            if let Err(e) = handle_upgraded(u, authority, dispatcher).await {
                                                tracing::error!("Upgraded handling error: {}", e);
                                            }
                                        },
                                        Err(e) => {
                                            tracing::error!("Upgrading error: {}", e)
                                        }
                                    }
                                });

                                Ok(res)
                            }
                        }))
                        .with_upgrades()
                        .await
                    {
                        tracing::error!("Stream handling error: {}", e);
                    }
                }.instrument(stream_span));
            }
        }
            .instrument(span)
            .await
    }

    fn name(&self) -> &str {
        "HttpInbound"
    }
}
