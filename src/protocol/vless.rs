use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt, ReadBuf};
use tokio_util::codec::Decoder;
use uuid::Uuid;
use bytes::{Buf, BufMut, BytesMut};
use crate::util::AsyncStreamExt;
use crate::{AsyncStream, TargetAddr};
use crate::error::VlessError;

type Result<T> = std::result::Result<T, VlessError>;

#[derive(Clone, Copy, Debug)]
pub enum VlessCommand {
    Tcp = 1,
    Udp = 2,
    Mux = 3
}

impl TryFrom<u8> for VlessCommand {
    type Error = VlessError;

    fn try_from(value: u8) -> std::result::Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Tcp),
            2 => Ok(Self::Udp),
            3 => Ok(Self::Mux),
            v => Err(VlessError::InvalidCommand(v))
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum VlessVersion {
    Zero = 0
}

impl TryFrom<u8> for VlessVersion {
    type Error = VlessError;

    fn try_from(value: u8) -> std::result::Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Zero),
            v => Err(VlessError::InvalidVersion(v))
        }
    }
}

struct VlessResponseDecoder;

impl Decoder for VlessResponseDecoder {
    type Item = ();
    type Error = std::io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> std::result::Result<Option<Self::Item>, Self::Error> {
        if src.len() < 2 {
            return Ok(None);
        }

        // We can not consume this byte what if
        // There is not enough data.
        let addons_len = src[1] as usize;

        let header_len = 2 + addons_len;
        if src.len() < header_len {
            return Ok(None); 
        }

        src.advance(header_len);

        Ok(Some(()))
    }
}

#[derive(Debug)]
pub struct VlessRequest {
    version: VlessVersion,
    id: Uuid,
    // addons: ProtoBuf,
    command: VlessCommand,
    target: TargetAddr
}

impl VlessRequest {
    pub fn new(version: VlessVersion, id: Uuid, command: VlessCommand, target: TargetAddr) -> Self {
        Self {
            version,
            id,
            command,
            target
        }
    }

    pub fn version(&self) -> &VlessVersion {
        &self.version
    }
    pub fn id(&self) -> &Uuid {
        &self.id
    }
    pub fn command(&self) -> &VlessCommand {
        &self.command
    }
    pub fn target(&self) -> &TargetAddr {
        &self.target
    }
}

pub struct VlessResponse {
    version: u8,
    // addons: ProtoBuf
}

pub struct VlessStream<S> {
    inner: S,
    buf: BytesMut,
    decoder: VlessResponseDecoder,
    is_handshake_done: bool
}

impl<S: AsyncStream> VlessStream<S> {
    pub fn new(stream: S) -> Self {
        Self {
            inner: stream,
            buf: BytesMut::with_capacity(256),
            decoder: VlessResponseDecoder,
            is_handshake_done: false
        }
    }

    pub async fn connect(&mut self, req: VlessRequest) -> Result<()> {
        let mut buf = BytesMut::with_capacity(22);
        buf.put_u8(req.version as u8);
        buf.put_slice(req.id.as_bytes());
        buf.put_u8(0); // addons len = 0
        buf.put_u8(req.command as u8);
        match req.target {
            TargetAddr::SocketAddr(SocketAddr::V4(addr)) => {
                buf.put_u16(addr.port());
                buf.put_u8(1);
                buf.put_slice(&addr.ip().octets());
            }
            TargetAddr::Domain(domain, port) => {
                buf.put_u16(port);
                buf.put_u8(2);
                buf.put_u8(domain.len() as u8);
                buf.put_slice(domain.as_bytes());
            }
            TargetAddr::SocketAddr(SocketAddr::V6(addr)) => {
                buf.put_u16(addr.port());
                buf.put_u8(3);
                buf.put_slice(&addr.ip().octets());
            }
        }
        
        self.inner.write_all_buf(&mut buf).await?;
        self.inner.flush().await?;

        Ok(())
    }
}

impl<S: AsyncStream> AsyncWrite for VlessStream<S> {
    fn poll_write(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

impl<S: AsyncStream> AsyncRead for VlessStream<S> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let this = &mut *self;

        if !this.is_handshake_done {
            loop {
                let n = {
                    let chunk = this.buf.chunk_mut();
                    let mut read_buf = ReadBuf::uninit(unsafe { chunk.as_uninit_slice_mut() });

                    match Pin::new(&mut this.inner).poll_read(cx, &mut read_buf) {
                        Poll::Ready(Ok(())) => read_buf.filled().len(),
                        Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                        Poll::Pending => return Poll::Pending,
                    }
                };

                if n == 0 {
                    return Poll::Ready(Err(std::io::Error::new(
                        std::io::ErrorKind::UnexpectedEof,
                        "EOF during handshake",
                    )));
                }

                unsafe { this.buf.advance_mut(n); }

                match this.decoder.decode(&mut this.buf) {
                    Ok(Some(_)) => {
                        this.is_handshake_done = true;

                        break; 
                    }
                    Ok(None) => {
                        continue;
                    }
                    Err(e) => return Poll::Ready(Err(e)),
                }
            }
        }
        
        // Place actual data back.
        if !this.buf.is_empty() {
            let len = std::cmp::min(buf.remaining(), this.buf.len());
            buf.put_slice(&this.buf[..len]);
            this.buf.advance(len);
            
            return Poll::Ready(Ok(()));
        }

        Pin::new(&mut this.inner).poll_read(cx, buf)
    }
}

pub struct VlessProtocol;

impl VlessProtocol {
    pub async fn accept(stream: &mut impl AsyncStream) -> Result<VlessRequest> {
        // version(1) + uuid(16) + addons_len(1) + command(1) + port(2) + addr_kind(1) = 22
        let mut header_buf = BytesMut::with_capacity(22);
        stream.read_buf_exact(&mut header_buf, 22).await?;
        let mut header = header_buf.freeze();

        let version = VlessVersion::try_from(header.get_u8())?;

        let id = header.copy_to_bytes(16);
        let id = Uuid::from_bytes(id.as_ref().try_into().unwrap());

        let addons_len = header.get_u8();
        if addons_len != 0 {
            // skip addons
            let mut addons = BytesMut::with_capacity(addons_len as usize);
            stream.read_buf_exact(&mut addons, addons_len as usize).await?;
        }

        let command = header.get_u8();
        let command = VlessCommand::try_from(command)?;

        let port = header.get_u16();

        let addr_kind = header.get_u8();
        let target = match addr_kind {
            1 => { // IPv4
                let mut ip = BytesMut::with_capacity(4);
                stream.read_buf_exact(&mut ip, 4).await?;
                let ip = ip.freeze();

                TargetAddr::SocketAddr(SocketAddr::new(IpAddr::V4(Ipv4Addr::from_octets(ip.as_ref().try_into().unwrap())), port))
            },
            2 => { // Domain
                let mut domain_len = BytesMut::with_capacity(1);
                stream.read_buf_exact(&mut domain_len, 1).await?;
                let domain_len = domain_len.freeze().get_u8();

                let mut domain = BytesMut::with_capacity(domain_len as usize);
                stream.read_buf_exact(&mut domain, domain_len as usize).await?;
                let domain = String::from_utf8_lossy(&domain).to_string();

                TargetAddr::Domain(domain, port)
            },
            3 => { // IPv6
                let mut ip = BytesMut::with_capacity(16);
                stream.read_buf_exact(&mut ip, 16).await?;
                let ip = ip.freeze();

                TargetAddr::SocketAddr(SocketAddr::new(IpAddr::V6(Ipv6Addr::from_octets(ip.as_ref().try_into().unwrap())), port))
            },
            v => return Err(VlessError::InvalidAddrKind(v))
        };

        let mut res = BytesMut::with_capacity(2);
        res.put_u8(VlessVersion::Zero as u8);
        res.put_u8(0);
        stream.write_all_buf(&mut res).await?;
        stream.flush().await?;

        Ok(VlessRequest {
            version,
            id,
            command,
            target
        })
    }

    pub async fn send_response(stream: &mut impl AsyncStream) -> Result<()> {
        let mut buf = BytesMut::with_capacity(2);
        buf.put_u8(VlessVersion::Zero as u8);
        buf.put_u8(0);
        stream.write_all_buf(&mut buf).await?;
        stream.flush().await?;

        Ok(())
    }
}
