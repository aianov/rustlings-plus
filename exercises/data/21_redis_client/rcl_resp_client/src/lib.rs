mod client;
mod codec;

use std::fmt;
use std::io;
use std::time::Duration;

pub use client::Client;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Simple(String),
    Error(String),
    Int(i64),
    Bulk(Vec<u8>),
    Null,
    Array(Vec<Value>),
}

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Protocol(String),
    Server { kind: String, message: String },
    Unexpected(Value),
    Timeout,
    Disconnected,
    Closed,
    InvalidUrl(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "ошибка ввода-вывода: {e}"),
            Error::Protocol(reason) => write!(f, "нарушение протокола: {reason}"),
            Error::Server { kind, message } => write!(f, "{kind}: {message}"),
            Error::Unexpected(value) => write!(f, "неожиданный ответ: {value:?}"),
            Error::Timeout => write!(f, "не уложились в срок"),
            Error::Disconnected => write!(f, "соединение оборвалось"),
            Error::Closed => write!(f, "клиент закрыт"),
            Error::InvalidUrl(reason) => write!(f, "плохой адрес: {reason}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Error {
        Error::Io(e)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone)]
pub struct Options {
    pub connect_timeout: Duration,
    pub command_timeout: Duration,
    pub reconnect_pause: Duration,
    pub max_in_flight: usize,
    pub max_bulk: usize,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            connect_timeout: Duration::from_secs(2),
            command_timeout: Duration::from_secs(2),
            reconnect_pause: Duration::from_millis(100),
            max_in_flight: 256,
            max_bulk: 64 << 20,
        }
    }
}
