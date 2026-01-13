use std::io::{Read, Write};
use std::task::{Poll, Context};
use std::pin::Pin;
use rustls::Connection;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use crate::AsyncStream;

pub struct SyncAdapter<'a, 'b, S> {
    inner: &'a mut S,
    cx: &'a mut Context<'b>
}

impl<'a, 'b, S: AsyncStream> Read for SyncAdapter<'a, 'b, S> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let mut read_buf = ReadBuf::new(buf);
        match Pin::new(&mut *self.inner).poll_read(self.cx, &mut read_buf) {
            Poll::Ready(Ok(())) => Ok(read_buf.filled().len()),
            Poll::Ready(Err(e)) => Err(e),
            Poll::Pending => Err(std::io::ErrorKind::WouldBlock.into())
        }
    }
}

impl<'a, 'b, S: AsyncStream> Write for SyncAdapter<'a, 'b, S> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match Pin::new(&mut *self.inner).poll_write(self.cx, buf) {
            Poll::Ready(Ok(n)) => Ok(n),
            Poll::Ready(Err(e)) => Err(e),
            Poll::Pending => Err(std::io::ErrorKind::WouldBlock.into())
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match Pin::new(&mut *self.inner).poll_flush(self.cx) {
            Poll::Ready(Ok(())) => Ok(()),
            Poll::Ready(Err(e)) => Err(e),
            Poll::Pending => Err(std::io::ErrorKind::WouldBlock.into())
        }
    }
}

pub struct TlsStream<S> {
    inner: S,
    session: Connection,
    eof: bool
}

impl<S: AsyncStream> TlsStream<S> {
    pub fn new(inner: S, session: Connection) -> Self {
        Self {
            inner,
            session,
            eof: false
        }
    }

    fn flush_tls(&mut self, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        let mut adapter = SyncAdapter { inner: &mut self.inner, cx };

        while self.session.wants_write() {
            match self.session.write_tls(&mut adapter) {
                Ok(0) => {
                    tracing::debug!("Reached EOF while writing");

                    self.eof = true;

                    break;
                },
                Ok(n) => tracing::debug!("Wrote {}B ciphertext to inner", n),
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => return Poll::Pending,
                Err(e) => return Poll::Ready(Err(e))
            }
        }

        Poll::Ready(Ok(()))
    }

    fn drive_tls_readable(&mut self, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match self.flush_tls(cx) {
            Poll::Ready(Ok(())) => {},
            Poll::Pending => return Poll::Pending,
            Poll::Ready(Err(e)) => return Poll::Ready(Err(e))
        };

        let mut adapter = SyncAdapter { inner: &mut self.inner, cx };
        match self.session.read_tls(&mut adapter) {
            Ok(0) => {
                tracing::debug!("Reached EOF while reading");

                self.eof = true;

                Poll::Ready(Ok(()))
            },
            Ok(n) => {
                tracing::debug!("Read {}B ciphertext from inner", n);

                self.session
                    .process_new_packets()
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

                self.flush_tls(cx)
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => Poll::Pending,
            Err(e) => Poll::Ready(Err(e))
        }
    }
}

impl<S: AsyncStream> AsyncRead for TlsStream<S> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        if self.eof {
            return Poll::Ready(Ok(()));
        }

        while self.session.is_handshaking() && !self.eof {
            match self.drive_tls_readable(cx) {
                Poll::Ready(Ok(())) => {}
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e))
            }
        }

        while !self.eof {
            match self.session.reader().read(buf.initialize_unfilled()) {
                Ok(0) => {},
                Ok(n) => {
                    tracing::debug!("Read {}B plaintext from rustls buf", n);
                    buf.advance(n);

                    return Poll::Ready(Ok(()));
                },
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {},
                Err(e) => return Poll::Ready(Err(e))
            }

            match self.drive_tls_readable(cx) {
                Poll::Ready(Ok(())) => {},
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e))
            }
        }

        Poll::Ready(Ok(()))
    }
}

impl<S: AsyncStream> AsyncWrite for TlsStream<S> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8]
    ) -> Poll<std::io::Result<usize>> {
        if self.eof {
            return Poll::Ready(Ok(0));
        }

        while self.session.is_handshaking() && !self.eof {
            match self.drive_tls_readable(cx) {
                Poll::Ready(Ok(())) => {}
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e))
            }
        }

        match self.flush_tls(cx) {
            Poll::Ready(Ok(_)) => {},
            Poll::Pending => {}, // do not prevent below code
            Poll::Ready(Err(e)) => return Poll::Ready(Err(e))
        }

        if buf.is_empty() {
            return Poll::Ready(Ok(0));
        }

        loop {
            match self.session.writer().write(buf) {
                Ok(0) => {
                    match self.flush_tls(cx) {
                        Poll::Ready(Ok(_)) => continue,
                        Poll::Pending => return Poll::Pending,
                        Poll::Ready(Err(e)) => return Poll::Ready(Err(e))
                    }
                },
                Ok(n) => {
                    tracing::debug!("Wrote {}B plaintext to rustls buf", n);

                    match self.flush_tls(cx) {
                        Poll::Ready(Ok(_)) => return Poll::Ready(Ok(n)),
                        Poll::Pending => return Poll::Ready(Ok(n)),
                        Poll::Ready(Err(e)) => return Poll::Ready(Err(e))
                    }
                },
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    match self.flush_tls(cx) {
                        Poll::Ready(Ok(_)) => continue,
                        Poll::Pending => return Poll::Pending,
                        Poll::Ready(Err(e)) => return Poll::Ready(Err(e))
                    }
                },
                Err(e) => return Poll::Ready(Err(e))
            }
        }
    }

    fn poll_flush(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>
    ) -> Poll<std::io::Result<()>> {
        if self.eof {
            return Poll::Ready(Ok(()));
        }

        match self.flush_tls(cx) {
            Poll::Ready(Ok(_)) => {}
            Poll::Pending => return Poll::Pending,
            Poll::Ready(Err(e)) => return Poll::Ready(Err(e))
        }

        Pin::new(&mut self.inner).poll_flush(cx)
    }
    
    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>
    ) -> Poll<std::io::Result<()>> {
        if self.eof {
            return Poll::Ready(Ok(()));
        }

        self.session.send_close_notify();

        match self.flush_tls(cx) {
            Poll::Ready(Ok(_)) => {}
            Poll::Pending => return Poll::Pending,
            Poll::Ready(Err(e)) => return Poll::Ready(Err(e))
        }

        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}