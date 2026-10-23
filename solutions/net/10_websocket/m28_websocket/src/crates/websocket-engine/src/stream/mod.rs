use std::fmt;
use std::io;
use std::time::Duration;

use bytes::BytesMut;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::time::{Instant, timeout, timeout_at};

use crate::assembler::{Assembler, Event, Message};
use crate::close::{Closing, Phase, parse_close};
use crate::frame::{Decoder, Frame, MaskGen, Opcode, ProtoError, Role, close_code, encode_frame};
use crate::handshake::WsConfig;

#[derive(Debug)]
pub enum WsError {
    Io(io::Error),
    Protocol(ProtoError),
    Closed,
    Handshake(String),
}

impl fmt::Display for WsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WsError::Io(..) => write!(f, "соединение оборвалось"),
            WsError::Protocol(error) => write!(f, "нарушен протокол WebSocket: {error}"),
            WsError::Closed => write!(f, "соединение уже закрыто"),
            WsError::Handshake(why) => write!(f, "рукопожатие не удалось: {why}"),
        }
    }
}

impl std::error::Error for WsError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            WsError::Io(error) => Some(error),
            WsError::Protocol(error) => Some(error),
            WsError::Closed | WsError::Handshake(..) => None,
        }
    }
}

impl From<io::Error> for WsError {
    fn from(e: io::Error) -> WsError {
        WsError::Io(e)
    }
}

pub struct WsStream<S> {
    io: S,
    buf: BytesMut,
    decoder: Decoder,
    assembler: Assembler,
    closing: Closing,
    role: Role,
    keys: MaskGen,
    close_code: Option<u16>,
    close_timeout: Duration,
}

impl<S: AsyncRead + AsyncWrite + Unpin> WsStream<S> {
    pub fn new(io: S, rest: Vec<u8>, role: Role, config: &WsConfig) -> WsStream<S> {
        WsStream {
            io,
            buf: BytesMut::from(&rest[..]),
            decoder: Decoder::new(role, config.max_message),
            assembler: Assembler::new(config.max_message),
            closing: Closing::new(),
            role,
            keys: MaskGen::new(),
            close_code: None,
            close_timeout: config.close_timeout,
        }
    }

    pub fn close_code(&self) -> Option<u16> {
        self.close_code
    }

    async fn write_frame(&mut self, frame: &Frame) -> Result<(), WsError> {
        let mask = match self.role {
            Role::Client => Some(self.keys.next_key()),
            Role::Server => None,
        };
        let mut out = BytesMut::with_capacity(frame.payload.len() + 14);
        encode_frame(frame, mask, &mut out);
        self.io.write_all(&out).await?;
        self.io.flush().await?;
        Ok(())
    }

    async fn finish(&mut self) {
        self.closing.mark_closed();
        let _ = self.io.shutdown().await;
        let limit = Duration::from_secs(1).min(self.close_timeout);
        let io = &mut self.io;
        let drain = async {
            let mut sink = [0u8; 4096];
            let mut total = 0usize;
            while let Ok(n) = io.read(&mut sink).await {
                total += n;
                if n == 0 || total > 1 << 20 {
                    break;
                }
            }
        };
        let _ = timeout(limit, drain).await;
    }

    async fn fail(&mut self, error: ProtoError) -> Result<Option<Message>, WsError> {
        let code = close_code(&error);
        self.close_code = Some(code);
        if let Some(frame) = self.closing.start(code, "") {
            let _ = self.write_frame(&frame).await;
        }
        self.finish().await;
        Err(WsError::Protocol(error))
    }

    async fn on_peer_close(&mut self, payload: Vec<u8>) -> Result<Option<Message>, WsError> {
        self.close_code = Some(match parse_close(&payload) {
            Ok(Some((code, _))) => code,
            Ok(None) => 1005,
            Err(ProtoError::InvalidUtf8) => 1007,
            Err(_) => 1002,
        });
        if let Some(reply) = self.closing.on_close(&payload) {
            let _ = self.write_frame(&reply).await;
        }
        self.finish().await;
        Ok(None)
    }

    pub async fn recv(&mut self) -> Result<Option<Message>, WsError> {
        loop {
            match self.decoder.decode(&mut self.buf) {
                Err(error) => return self.fail(error).await,
                Ok(Some(frame)) => match self.assembler.push(frame) {
                    Err(error) => return self.fail(error).await,
                    Ok(None) => {}
                    Ok(Some(Event::Message(message))) => {
                        if self.closing.accepts_data() {
                            return Ok(Some(message));
                        }
                    }
                    Ok(Some(Event::Control(frame))) => match frame.opcode {
                        Opcode::Ping => {
                            if self.closing.accepts_data() {
                                self.write_frame(&Frame::new(Opcode::Pong, &frame.payload))
                                    .await?;
                            }
                        }
                        Opcode::Close => return self.on_peer_close(frame.payload).await,
                        _ => {}
                    },
                },
                Ok(None) => {
                    if self.io.read_buf(&mut self.buf).await? == 0 {
                        let clean = self.closing.phase() != Phase::Open;
                        self.closing.mark_closed();
                        return if clean {
                            Ok(None)
                        } else {
                            Err(WsError::Io(io::ErrorKind::UnexpectedEof.into()))
                        };
                    }
                }
            }
        }
    }

    pub async fn send(&mut self, message: Message) -> Result<(), WsError> {
        if !self.closing.accepts_data() {
            return Err(WsError::Closed);
        }
        let frame = match message {
            Message::Text(text) => Frame::new(Opcode::Text, text.as_bytes()),
            Message::Binary(bytes) => Frame::new(Opcode::Binary, &bytes),
        };
        self.write_frame(&frame).await
    }

    pub async fn ping(&mut self, payload: &[u8]) -> Result<(), WsError> {
        if payload.len() > 125 {
            return Err(WsError::Protocol(ProtoError::ControlTooLong));
        }
        if !self.closing.accepts_data() {
            return Err(WsError::Closed);
        }
        self.write_frame(&Frame::new(Opcode::Ping, payload)).await
    }

    pub async fn close(&mut self, code: u16, reason: &str) -> Result<(), WsError> {
        let Some(frame) = self.closing.start(code, reason) else {
            return Ok(());
        };
        self.write_frame(&frame).await?;
        let deadline = Instant::now() + self.close_timeout;
        let waiting = async {
            loop {
                match self.decoder.decode(&mut self.buf) {
                    Ok(Some(frame)) => {
                        if frame.opcode == Opcode::Close {
                            self.close_code = Some(match parse_close(&frame.payload) {
                                Ok(Some((code, _))) => code,
                                _ => 1005,
                            });
                            break;
                        }
                    }
                    Err(_) => break,
                    Ok(None) => match self.io.read_buf(&mut self.buf).await {
                        Ok(0) | Err(_) => break,
                        Ok(_) => {}
                    },
                }
            }
        };
        let _ = timeout_at(deadline, waiting).await;
        self.finish().await;
        Ok(())
    }
}
