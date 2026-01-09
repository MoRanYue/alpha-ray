use std::net::SocketAddr;
use http_body_util::{BodyExt, Empty};
use hyper::body::Bytes;
use hyper::{Method, Request, StatusCode};
use hyper_util::rt::TokioIo;
use tokio::net::TcpStream;
use crate::{AsyncStream, TargetAddr};
use crate::error::AlphaRayError;
use super::Outbound;

pub struct HttpOutbound {
    server_addr: SocketAddr
}

impl HttpOutbound {
    pub fn new(server_addr: SocketAddr) -> Self {
        Self { server_addr }
    }
}

#[async_trait::async_trait]
impl Outbound for HttpOutbound {
    async fn connect(&self, target: TargetAddr) -> crate::Result<Box<dyn AsyncStream>> {
        let target_addr = match target {
            TargetAddr::SocketAddr(addr) => addr,
            TargetAddr::Domain(name, port) => {
                tokio::net::lookup_host((name.as_str(), port))
                    .await?
                    .into_iter()
                    .next()
                    .ok_or(AlphaRayError::Other(anyhow::anyhow!("Empty DNS response")))?
            }
        };

        let stream = TcpStream::connect(self.server_addr).await?;

        let io = TokioIo::new(stream);

        let (mut sender, conn) = hyper::client::conn::http1::handshake(io).await?;

        tokio::spawn(async move {
            if let Err(e) = conn.with_upgrades().await {
                tracing::error!("Connection error: {}", e);
            }
        });

        let req = Request::builder()
            .method(Method::CONNECT)
            .uri(target_addr.to_string())
            .header("Host", target_addr.to_string())
            .body(Empty::<Bytes>::new())?;

        let res = sender.send_request(req).await?;
        if res.status() == StatusCode::OK {
            let upgraded = hyper::upgrade::on(res).await?;

            let s = Box::new(TokioIo::new(upgraded)) as Box<dyn AsyncStream>;

            Ok(s)
        }
        else {
            Err(AlphaRayError::Other(anyhow::anyhow!("server responded with status code `{}`", res.status())))
        }
    }

    fn name(&self) -> &str {
        "HttpOutbound"
    }
}
