use fast_socks5::SocksError;
use thiserror::Error;
use tokio::task::JoinError;

#[derive(Debug, Error)]
pub enum VlessError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Invalid version `{0}`")]
    InvalidVersion(u8),
    #[error("Invalid command `{0}`")]
    InvalidCommand(u8),
    #[error("Invalid address kind `{0}`")]
    InvalidAddrKind(u8)
}

#[derive(Debug, Error)]
pub enum AlphaRayError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("SOCKS error: {0}")]
    Socks(#[from] SocksError),
    #[error("HTTP error: {0}")]
    Http(#[from] hyper::http::Error),
    #[error("HTTP stream error: {0}")]
    HttpStream(#[from] hyper::Error),
    #[error("VLESS error: {0}")]
    Vless(#[from] VlessError),
    #[error("TLS error: {0}")]
    Tls(#[from] rustls::Error),
    #[error("Certificate error: {0}")]
    Certificate(#[from] rustls::pki_types::pem::Error),
    #[error("Thread error: {0}")]
    Thread(#[from] JoinError),
    #[error("Permission error")]
    Permission,
    #[error("Timed out")]
    Timeout,
    #[error("Failed to connect")]
    Connection,
    #[error(transparent)]
    Other(#[from] anyhow::Error)
}
