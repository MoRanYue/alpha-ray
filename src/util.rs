use std::net::{SocketAddr, ToSocketAddrs};
use std::fmt::{Display, Formatter};
use std::str::FromStr;
use tokio::io::{AsyncRead, AsyncWrite};
use crate::error::AlphaRayError;

pub type Result<T> = std::result::Result<T, AlphaRayError>;

#[derive(Clone, Debug)]
pub enum TargetAddr {
    SocketAddr(SocketAddr),
    Domain(String, u16)
}

impl Display for TargetAddr {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            TargetAddr::SocketAddr(a) => write!(f, "{}", a),
            TargetAddr::Domain(d, p) => write!(f, "{}:{}", d, p),
        }
    }
}

impl From<fast_socks5::util::target_addr::TargetAddr> for TargetAddr {
    fn from(value: fast_socks5::util::target_addr::TargetAddr) -> Self {
        match value {
            fast_socks5::util::target_addr::TargetAddr::Ip(a) => Self::SocketAddr(a),
            fast_socks5::util::target_addr::TargetAddr::Domain(d, p) => Self::Domain(d, p),
        }
    }
}

impl FromStr for TargetAddr {
    type Err = AlphaRayError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        if let Ok(a) = s.parse::<SocketAddr>() {
            return Ok(TargetAddr::SocketAddr(a))
        }

        if let Some(i) = s.rfind(':') {
            let (host, port) = s.split_at(i);
            let port = port.trim_start_matches(':').parse::<u16>()
                .map_err(|e| anyhow::anyhow!(e))?;

            return Ok(TargetAddr::Domain(host.to_string(), port))
        }

        Err(AlphaRayError::Other(anyhow::anyhow!("invalid addr: {}", s)))
    }
}

pub trait ToTargetAddr {
    fn to_target_addr(self) -> crate::Result<TargetAddr>;
}

impl<T: ToSocketAddrs> ToTargetAddr for T {
    fn to_target_addr(self) -> crate::Result<TargetAddr> {
        self.to_socket_addrs()
            .map_err(Into::into)
            .and_then(|mut a| a.next().ok_or(AlphaRayError::Other(anyhow::anyhow!("Address does not exist"))))
            .map(|a| TargetAddr::SocketAddr(a))
    }
}

impl ToTargetAddr for TargetAddr {
    fn to_target_addr(self) -> crate::Result<TargetAddr> {
        Ok(self)
    }
}

pub trait AsyncStream: AsyncRead + AsyncWrite + Send + Unpin {}
impl<T: AsyncRead + AsyncWrite + Send + Unpin> AsyncStream for T {}