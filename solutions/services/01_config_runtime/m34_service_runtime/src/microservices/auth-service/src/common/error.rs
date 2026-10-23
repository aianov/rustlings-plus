use std::error::Error as StdError;
use std::fmt;
use std::io;

use service_runtime::{ConfigError, Fatal, Halt, StartupError};

#[derive(Debug)]
pub enum Error {
    Config(ConfigError),
    Journal(journal::JournalError),
    Bind { addr: String, source: io::Error },
    Startup(StartupError),
    Serve(tonic::transport::Error),
    Background(Fatal),
    Tls(tonic::transport::Error),
    Certs(io::Error),
    Upstream(tonic::transport::Error),
    Io(io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

impl From<journal::JournalError> for Error {
    fn from(error: journal::JournalError) -> Error {
        Error::Journal(error)
    }
}

impl From<ConfigError> for Error {
    fn from(error: ConfigError) -> Error {
        Error::Config(error)
    }
}

impl From<StartupError> for Error {
    fn from(error: StartupError) -> Error {
        Error::Startup(error)
    }
}

impl From<Halt<Error>> for Error {
    fn from(halt: Halt<Error>) -> Error {
        match halt {
            Halt::Startup(error) => Error::Startup(error),
            Halt::Serve(error) => error,
            Halt::Fatal(fatal) => Error::Background(fatal),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Config(..) => write!(f, "не запускаюсь"),
            Error::Journal(..) => write!(f, "журнал учётных записей не поднялся"),
            Error::Bind { addr, .. } => write!(f, "не занять адрес {addr}"),
            Error::Startup(..) => write!(f, "не стартую без зависимостей"),
            Error::Serve(..) => write!(f, "gRPC-сервер упал"),
            Error::Background(..) => write!(f, "упала критичная фоновая задача"),
            Error::Tls(..) => write!(f, "TLS не настроился"),
            Error::Certs(..) => write!(f, "нет сертификатов сервиса"),
            Error::Upstream(..) => write!(f, "плохой адрес user-service"),
            Error::Io(..) => write!(f, "ошибка сокета"),
        }
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Error::Config(error) => Some(error),
            Error::Journal(error) => Some(error),
            Error::Bind { source, .. } => Some(source),
            Error::Startup(error) => Some(error),
            Error::Background(error) => Some(error),
            Error::Serve(error) | Error::Upstream(error) | Error::Tls(error) => Some(error),
            Error::Io(error) | Error::Certs(error) => Some(error),
        }
    }
}
