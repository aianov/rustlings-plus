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
            WsError::Io(e) => write!(f, "io: {e}"),
            WsError::Protocol(e) => write!(f, "protocol: {e:?}"),
            WsError::Closed => write!(f, "closed"),
            WsError::Handshake(m) => write!(f, "handshake: {m}"),
        }
    }
}

impl std::error::Error for WsError {}

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


    pub async fn recv(&mut self) -> Result<Option<Message>, WsError> {
        todo!()
    }

    pub async fn send(&mut self, message: Message) -> Result<(), WsError> {
        todo!()
    }

    pub async fn ping(&mut self, payload: &[u8]) -> Result<(), WsError> {
        todo!()
    }

    pub async fn close(&mut self, code: u16, reason: &str) -> Result<(), WsError> {
        todo!()
    }
}
