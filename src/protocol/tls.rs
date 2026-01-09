use std::task::{Context, Poll};
use std::io::{Read, Write};
use std::pin::Pin;
use rustls::Connection;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use crate::AsyncStream;

pub struct SyncAdapter<'a, 'b, S> {
    pub io: &'a mut S,
    pub cx: &'a mut Context<'b>
}

impl<'a, 'b, S: AsyncStream> Read for SyncAdapter<'a, 'b, S> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let mut read_buf = ReadBuf::new(buf);
        match Pin::new(&mut self.io).poll_read(self.cx, &mut read_buf) {
            Poll::Ready(Ok(_)) => Ok(read_buf.filled().len()),
            Poll::Ready(Err(e)) => Err(e),
            Poll::Pending => Err(std::io::ErrorKind::WouldBlock.into()),
        }
    }
}

impl<'a, 'b, S: AsyncStream> Write for SyncAdapter<'a, 'b, S> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match Pin::new(&mut self.io).poll_write(self.cx, buf) {
            Poll::Ready(r) => r,
            Poll::Pending => Err(std::io::ErrorKind::WouldBlock.into()),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match Pin::new(&mut self.io).poll_flush(self.cx) {
            Poll::Ready(r) => r,
            Poll::Pending => Err(std::io::ErrorKind::WouldBlock.into()),
        }
    }
}

pub struct TlsStream<S> {
    inner: S,
    session: Connection
}

impl<S: AsyncStream> TlsStream<S> {
    pub fn new(inner: S, session: Connection) -> Self {
        Self { inner, session }
    }
}

impl<S: AsyncStream> AsyncRead for TlsStream<S> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let Self { inner, session } = &mut *self;
        let mut adapter = SyncAdapter { io: inner, cx };

        loop {
            // rustls buf plaintext to buf
            // consume rustls buf plaintext first
            match session.reader().read(buf.initialize_unfilled()) {
                Ok(n) if n > 0 => {
                    buf.advance(n);

                    return Poll::Ready(Ok(()));
                }
                Ok(_) => {},
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {},
                Err(e) => return Poll::Ready(Err(e))
            }

            // inner cipher to rustls buf
            match session.read_tls(&mut adapter) {
                Ok(0) => { // EOF
                    // avoid data loss
                    match session.reader().read(buf.initialize_unfilled()) {
                        Ok(n) if n > 0 => {
                            buf.advance(n);

                            return Poll::Ready(Ok(()));
                        }
                        _ => return Poll::Ready(Ok(()))
                    }
                },
                Ok(_) => { // Has read data
                    session.process_new_packets()
                        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

                    continue;
                },
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => { // Empty
                    match session.write_tls(&mut adapter) {
                        Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {},
                        Err(e) => return Poll::Ready(Err(e)),
                        Ok(_) => {}
                    }

                    return Poll::Pending;
                },
                Err(e) => return Poll::Ready(Err(e))
            }
        }
    }
}

impl<S: AsyncStream> AsyncWrite for TlsStream<S> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let Self { inner, session } = &mut *self;
        let mut adapter = SyncAdapter { io: inner, cx };

        // buf plaintext to rustls buf
        let has_written = session.writer().write(buf)?;

        // rustls buf cipher to inner
        while session.wants_write() {
            match session.write_tls(&mut adapter) {
                Ok(_) => {}
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) => return Poll::Ready(Err(e)),
            }
        }
        
        Poll::Ready(Ok(has_written))
    }

    fn poll_flush(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>
    ) -> Poll<std::io::Result<()>> {
        let Self { inner, session } = &mut *self;
        {
            let mut adapter = SyncAdapter { io: inner, cx };

            // flush rustls buf cipher to inner first
            while session.wants_write() {
                match session.write_tls(&mut adapter) {
                    Ok(_) => {}
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => return Poll::Pending,
                    Err(e) => return Poll::Ready(Err(e)),
                }
            }
        }

        Pin::new(inner).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>
    ) -> Poll<std::io::Result<()>> {
        let Self { inner, session } = &mut *self;
        {
            let mut adapter = SyncAdapter { io: inner, cx };

            session.send_close_notify();

            // write close notify to inner
            while session.wants_write() {
                match session.write_tls(&mut adapter) {
                    Ok(_) => {}
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => return Poll::Pending,
                    Err(e) => return Poll::Ready(Err(e)),
                }
            }
        }

        Pin::new(inner).poll_shutdown(cx)
    }
}