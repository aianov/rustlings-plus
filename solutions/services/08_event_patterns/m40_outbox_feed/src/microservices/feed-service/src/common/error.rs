use std::error::Error as StdError;
use std::fmt;
use std::io;

use nats_engine::BusError;
use service_runtime::{ConfigError, Fatal, Halt, StartupError};

#[derive(Debug)]
pub enum Error {
    Config(ConfigError),
    Bind { addr: String, source: io::Error },
    Bus(BusError),
    Startup(StartupError),
    Serve(tonic::transport::Error),
    Background(Fatal),
    Tls(tonic::transport::Error),
    Certs(io::Error),
    Io(io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

impl From<ConfigError> for Error {
    fn from(error: ConfigError) -> Error {
        Error::Config(error)
    }
}

impl From<Halt<tonic::transport::Error>> for Error {
    fn from(halt: Halt<tonic::transport::Error>) -> Error {
        match halt {
            Halt::Startup(error) => Error::Startup(error),
            Halt::Serve(error) => Error::Serve(error),
            Halt::Fatal(fatal) => Error::Background(fatal),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Config(..) => write!(f, "не запускаюсь"),
            Error::Bind { addr, .. } => write!(f, "не занять адрес {addr}"),
            Error::Bus(..) => write!(f, "NATS не отвечает"),
            Error::Startup(..) => write!(f, "не догнал stream событий"),
            Error::Serve(..) => write!(f, "gRPC-сервер упал"),
            Error::Background(..) => write!(f, "упала критичная фоновая задача"),
            Error::Tls(..) => write!(f, "TLS не настроился"),
            Error::Certs(..) => write!(f, "нет сертификатов сервиса"),
            Error::Io(..) => write!(f, "ошибка сокета"),
        }
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Error::Config(error) => Some(error),
            Error::Bind { source, .. } => Some(source),
            Error::Bus(error) => Some(error),
            Error::Startup(error) => Some(error),
            Error::Background(error) => Some(error),
            Error::Serve(error) | Error::Tls(error) => Some(error),
            Error::Io(error) | Error::Certs(error) => Some(error),
        }
    }
}
