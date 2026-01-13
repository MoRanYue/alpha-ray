use std::pin::Pin;
use std::ops::RangeInclusive;
use std::task::{Context, Poll};
use std::io::{Read, Write};
use bytes::{Buf, BufMut, BytesMut};
use fastrand::Rng;
use rustls::Connection;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use uuid::Uuid;
use crate::AsyncStream;

pub struct SyncAdapter<'a, 'b, S> { pub io: &'a mut S, pub cx: &'a mut Context<'b> } impl<'a, 'b, S: AsyncStream> Read for SyncAdapter<'a, 'b, S> { fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> { let mut read_buf = ReadBuf::new(buf); match Pin::new(&mut *self.io).poll_read(self.cx, &mut read_buf) { Poll::Ready(Ok(_)) => Ok(read_buf.filled().len()), Poll::Ready(Err(e)) => Err(e), Poll::Pending => Err(std::io::ErrorKind::WouldBlock.into()), } } } impl<'a, 'b, S: AsyncStream> Write for SyncAdapter<'a, 'b, S> { fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> { match Pin::new(&mut *self.io).poll_write(self.cx, buf) { Poll::Ready(Ok(0)) if !buf.is_empty() => Err(std::io::ErrorKind::WriteZero.into()), Poll::Ready(r) => r, Poll::Pending => Err(std::io::ErrorKind::WouldBlock.into()), } } fn flush(&mut self) -> std::io::Result<()> { match Pin::new(&mut *self.io).poll_flush(self.cx) { Poll::Ready(r) => r, Poll::Pending => Err(std::io::ErrorKind::WouldBlock.into()), } } }

#[derive(Debug)]
pub enum VisionCommand {
    ContinuePadding = 0,
    EndPadding = 1,
    StartDirectCopy = 2
}

impl TryFrom<u8> for VisionCommand {
    type Error = std::io::Error;

    fn try_from(value: u8) -> std::result::Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::ContinuePadding),
            1 => Ok(Self::EndPadding),
            2 => Ok(Self::StartDirectCopy),
            v => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid command"))
        }
    }
}

pub enum VisionReadingStage {
    Id, // server
    Header,
    Content,
    Padding
}

pub struct VisionState {
    ids: Vec<[u8; 16]>,
    rng: Rng,
    padding_len_range: RangeInclusive<u16>,
    long_padding_threshold: u16,
    long_padding_base_len: u16,
    long_padding_len_range: RangeInclusive<u16>,
    is_server: bool,
    has_sent_id: bool, // client
    is_safe_tls_ver: bool,
    has_seen_tls_app_data: bool,
    stage: VisionReadingStage,
    remaining_content_len: usize,
    remaining_padding_len: usize,
    command: VisionCommand,
    read_direct_copy: bool,
    write_direct_copy: bool,
    can_start_direct_copy: bool,
}

impl VisionState {
    fn new(
        ids: Vec<[u8; 16]>,
        is_server: bool,
        padding_len_range: RangeInclusive<u16>,
        long_padding_threshold: u16,
        long_padding_base_len: u16,
        long_padding_len_range: RangeInclusive<u16>
    ) -> Self {
        Self {
            ids,
            rng: Rng::new(),
            padding_len_range,
            long_padding_threshold,
            long_padding_base_len,
            long_padding_len_range,
            is_server,
            has_sent_id: false,
            is_safe_tls_ver: false,
            has_seen_tls_app_data: false,
            stage: VisionReadingStage::Id,
            remaining_content_len: 0,
            remaining_padding_len: 0,
            command: VisionCommand::ContinuePadding,
            read_direct_copy: false,
            write_direct_copy: false,
            can_start_direct_copy: false
        }
    }
}

pub struct VisionTlsStream<S> {
    inner: S,
    session: Connection,
    state: VisionState,
    read_buf: BytesMut, // vision(data) from inner
    write_buf: BytesMut, // vision(data) to inner
    server_hello_buf: BytesMut // server
}

const MAX_TLS_SNIFFING_LEN: usize = 128;

impl<S: AsyncStream> VisionTlsStream<S> {
    pub fn new(
        inner: S,
        session: Connection,
        ids: &[Uuid],
        padding_len_range: RangeInclusive<u16>,
        long_padding_threshold: u16,
        long_padding_base_len: u16,
        long_padding_len_range: RangeInclusive<u16>
    ) -> Self {
        let is_server = match session {
            Connection::Server(_) => true,
            Connection::Client(_) => false,
        };

        Self {
            inner,
            session,
            state: VisionState::new(
                ids.iter().map(|i| i.into_bytes()).collect(),
                is_server,
                padding_len_range,
                long_padding_threshold,
                long_padding_base_len,
                long_padding_len_range,
            ),
            read_buf: BytesMut::with_capacity(8192),
            write_buf: BytesMut::with_capacity(8192),
            server_hello_buf: BytesMut::with_capacity(MAX_TLS_SNIFFING_LEN)
        }
    }

    fn unframe_read_buf(&mut self, buf: &mut ReadBuf<'_>) -> std::io::Result<usize> {
        if buf.remaining() == 0 {
            return Ok(0);
        }

        tracing::debug!("read_buf: {:02X?}", self.read_buf.as_ref());

        let mut n = 0;
        loop {
            if self.read_buf.is_empty() {
                break;
            }

            if self.state.read_direct_copy {
                let available_len = self.read_buf.len();
                let len_to_write = std::cmp::min(available_len, buf.remaining());
                if len_to_write == 0 {
                    break;
                }

                buf.put_slice(&self.read_buf.split_to(len_to_write));
                n += len_to_write;

                continue;
            }

            match self.state.stage {
                VisionReadingStage::Id => {
                    if self.state.is_server {
                        if self.read_buf.len() < 16 {
                            break;
                        }

                        if !self.state.ids.contains(self.read_buf[..16].try_into().unwrap()) {
                            self.state.read_direct_copy = true;

                            continue;
                        }

                        tracing::debug!(
                            "ID: {}",
                            Uuid::from_bytes(self.read_buf[..16].try_into().unwrap())
                        );

                        self.read_buf.advance(16);
                    }

                    self.state.stage = VisionReadingStage::Header;
                },
                VisionReadingStage::Header => {
                    if self.read_buf.len() < 5 {
                        break;
                    }
                    
                    let command = VisionCommand::try_from(self.read_buf[0])?;
                    let content_len = u16::from_be_bytes(self.read_buf[1..=2].try_into().unwrap());
                    let padding_len = u16::from_be_bytes(self.read_buf[3..=4].try_into().unwrap());
                    self.read_buf.advance(5);

                    tracing::debug!(
                        "Command: {:?}, Content length: {}B, Padding length: {}B",
                        command,
                        content_len,
                        padding_len
                    );

                    self.state.command = command;
                    self.state.remaining_content_len = content_len as usize;
                    self.state.remaining_padding_len = padding_len as usize;
                    
                    self.state.stage = VisionReadingStage::Content;
                },
                VisionReadingStage::Content => {
                    let needed_len = self.state.remaining_content_len;
                    if needed_len == 0 {
                        self.state.stage = VisionReadingStage::Padding;

                        continue;
                    }

                    let available_len = self.read_buf.len();
                    let len_to_process = std::cmp::min(available_len, needed_len);
                    let len_to_write = std::cmp::min(len_to_process, buf.remaining());

                    if len_to_write > 0 {
                        tracing::debug!("Reading {}B content", len_to_write);
                        buf.put_slice(&self.read_buf.split_to(len_to_write));
                        self.state.remaining_content_len -= len_to_write;
                        n += len_to_write;
                    }

                    if buf.remaining() == 0 && self.state.remaining_content_len > 0 {
                        return Ok(n);
                    }

                    if self.state.remaining_content_len == 0 {
                        self.state.stage = VisionReadingStage::Padding;
                    }
                },
                VisionReadingStage::Padding => {
                    let needed_len = self.state.remaining_padding_len;
                    let available_len = self.read_buf.len();
                    let len_to_drop = std::cmp::min(available_len, needed_len);

                    tracing::debug!("Dropping {}B padding", len_to_drop);
                    self.read_buf.advance(len_to_drop);
                    self.state.remaining_padding_len -= len_to_drop;

                    if self.state.remaining_padding_len == 0 {
                        match self.state.command {
                            VisionCommand::ContinuePadding => self.state.stage = VisionReadingStage::Header,
                            VisionCommand::EndPadding => {
                                tracing::debug!("Start direct copy when reading");
                                self.state.read_direct_copy = true;

                                self.state.stage = VisionReadingStage::Header;
                            },
                            VisionCommand::StartDirectCopy => {
                                tracing::debug!("Start direct copy when reading");
                                self.state.read_direct_copy = true;
                                // self.state.direct_copy = true;

                                self.state.stage = VisionReadingStage::Header;
                            }
                        }
                    }
                    else {
                        break;
                    }
                },
            }
        }

        Ok(n)
    }

    fn frame_buf(&mut self, buf: &[u8]) {
        if self.state.write_direct_copy {
            self.write_buf.extend_from_slice(buf);

            return;
        }

        if !self.state.is_server && !self.state.has_sent_id {
            self.write_buf.extend_from_slice(&self.state.ids[0]);
            self.state.has_sent_id = true;
        }

        let mut command = VisionCommand::ContinuePadding;
        if self.state.can_start_direct_copy &&
            buf.len() >= 3 &&
            buf.starts_with(&[0x17, 0x03, 0x03])
        {
            tracing::debug!("Sniffed TLS Application Data");
            self.state.has_seen_tls_app_data = true;
            self.state.write_direct_copy = true;
            command = VisionCommand::StartDirectCopy;
        }

        if self.server_hello_buf.is_empty() {
            tracing::debug!("Sniffing TLS handshake packets in slice: {:02X?}", buf);
            if buf.len() >= 6 && buf.starts_with(&[0x16, 0x03, 0x01]) {
                self.state.is_safe_tls_ver = true;
                self.server_hello_buf.put_slice(&buf);

                if buf[5] == 0x01 {
                    tracing::debug!("Sniffed ClientHello");
                    self.state.can_start_direct_copy = true;
                }
                else if buf[5] == 0x02 {
                    tracing::debug!("Sniffed ServerHello");
                }
            }
        }
        else {
            if self.server_hello_buf.len() < MAX_TLS_SNIFFING_LEN {
                let len_to_copy = std::cmp::min(MAX_TLS_SNIFFING_LEN - self.server_hello_buf.len(), buf.len());
                self.server_hello_buf.put_slice(&buf[..len_to_copy]);
            }

            let session_id_len;
            if self.server_hello_buf.len() >= 44 {
                session_id_len = self.server_hello_buf[43];
                if self.server_hello_buf.len() >= 47 + session_id_len as usize {
                    // do not start direct copy when TLS_AES_128_CCM_8_SHA256
                    let idx = 44 + session_id_len as usize;
                    self.state.can_start_direct_copy = u16::from_be_bytes(self.server_hello_buf[idx..=(idx + 1)].try_into().unwrap())
                        != 0x1305;
                    self.server_hello_buf.clear();
                }
            }
        }

        let content_len = buf.len();
        let padding_len = if buf.len() <= self.state.long_padding_threshold as usize {
            (
                self.state.rng.u16(self.state.long_padding_len_range.clone())
                + self.state.long_padding_base_len
            ).saturating_sub(content_len as u16)
        }
        else {
            self.state.rng.u16(self.state.padding_len_range.clone())
        };

        self.write_buf.put_u8(command as u8);
        self.write_buf.put_u16(content_len as u16);
        self.write_buf.put_u16(padding_len);

        self.write_buf.extend_from_slice(buf);
        if padding_len > 0 {
            self.write_buf.put_bytes(0, padding_len as usize);
        }
    }
}

impl<S: AsyncStream> AsyncRead for VisionTlsStream<S> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        loop {
            // read buf plaintext to buf
            let n = match self.unframe_read_buf(buf) {
                Ok(n) => n,
                Err(e) => return Poll::Ready(Err(e))
            };
            if n > 0 {
                return Poll::Ready(Ok(()));
            }

            if buf.remaining() == 0 {
                return Poll::Ready(Ok(()));
            }

            let Self { inner, session, state, read_buf, .. } = &mut *self;
            let mut adapter = SyncAdapter { io: inner, cx };

            // rustls buf vision to read buf
            let mut temp_buf = [0; 4096];
            match session.reader().read(&mut temp_buf) {
                Ok(0) => {},
                Ok(n) => {
                    read_buf.extend_from_slice(&temp_buf[..n]);

                    continue;
                },
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {},
                Err(e) => return Poll::Ready(Err(e))
            }

            // inner cipher to rustls buf
            let read_cipher_len = match session.read_tls(&mut adapter) {
                Ok(n) => {
                    session.process_new_packets()
                        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

                    n
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => return Poll::Pending,
                Err(e) => return Poll::Ready(Err(e))
            };

            match session.reader().read(&mut temp_buf) {
                Ok(0) if read_cipher_len == 0 => return Poll::Ready(Ok(())),
                Ok(0) => continue,
                Ok(n) => {
                    read_buf.extend_from_slice(&temp_buf[..n]);

                    continue;
                },
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => return Poll::Pending,
                Err(e) => return Poll::Ready(Err(e))
            }
        }
    }
}

impl<S: AsyncStream> AsyncWrite for VisionTlsStream<S> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, std::io::Error>> {
        {
            let Self { inner, session, write_buf, .. } = &mut *self;
            let mut adapter = SyncAdapter { io: inner, cx };

            // write buf vision to rustls buf
            while !write_buf.is_empty() {
                match session.writer().write(&write_buf) {
                    Ok(0) => {
                        // rustls buf cipher to inner
                        match session.write_tls(&mut adapter) {
                            Ok(_) => {},
                            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => return Poll::Pending,
                            Err(e) => return Poll::Ready(Err(e))
                        }
                    },
                    Ok(n) => write_buf.advance(n),
                    Err(e) => return Poll::Ready(Err(e))
                }
            }
        }

        let len_to_write = std::cmp::min(buf.len(), u16::MAX as usize);
        // buf plaintext to write buf
        self.frame_buf(&buf[..len_to_write]);

        let Self { inner, session, write_buf, .. } = &mut *self;
        let mut adapter = SyncAdapter { io: inner, cx };

        // write buf vision to rustls buf
        while !write_buf.is_empty() {
            match session.writer().write(&write_buf) {
                Ok(n) => write_buf.advance(n),
                Err(e) => return Poll::Ready(Err(e))
            }
        }

        // rustls buf cipher to inner
        while session.wants_write() {
            match session.write_tls(&mut adapter) {
                Ok(_) => {},
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) => return Poll::Ready(Err(e))
            }
        }

        Poll::Ready(Ok(len_to_write))
    }

    fn poll_flush(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>
    ) -> Poll<Result<(), std::io::Error>> {
        let Self { inner, session, .. } = &mut *self;
        let mut adapter = SyncAdapter { io: inner, cx };

        // rustls buf cipher to inner
        while session.wants_write() {
            match session.write_tls(&mut adapter) {
                Ok(_) => {},
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => return Poll::Pending,
                Err(e) => return Poll::Ready(Err(e))
            }
        }

        Pin::new(inner).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>
    ) -> Poll<Result<(), std::io::Error>> {
        let Self { inner, session, .. } = &mut *self;
        let mut adapter = SyncAdapter { io: inner, cx };
        
        session.send_close_notify();

        // rustls buf cipher to inner
        while session.wants_write() {
            match session.write_tls(&mut adapter) {
                Ok(_) => {},
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => return Poll::Pending,
                Err(e) => return Poll::Ready(Err(e)),
            }
        }

        Pin::new(inner).poll_shutdown(cx)
    }
}