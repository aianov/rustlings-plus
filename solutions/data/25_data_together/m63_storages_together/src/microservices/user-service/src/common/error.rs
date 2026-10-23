use std::error::Error as StdError;
use std::fmt;
use std::io;

use service_runtime::{ConfigError, Fatal, Halt, StartupError};

#[derive(Debug)]
pub enum Error {
    Config(ConfigError),
    Journal(service_runtime::StorageError),
    Bind { addr: String, source: io::Error },
    Startup(StartupError),
    Serve(tonic::transport::Error),
    Background(Fatal),
    Tls(tonic::transport::Error),
    Certs(io::Error),
    Io(io::Error),
    Bus(nats_engine::BusError),
    Db(db_engine::DbError),
    NoStorage,
}

pub type Result<T> = std::result::Result<T, Error>;

impl From<service_runtime::StorageError> for Error {
    fn from(error: service_runtime::StorageError) -> Error {
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
            Error::Journal(..) => write!(f, "журнал профилей не поднялся"),
            Error::Bind { addr, .. } => write!(f, "не занять адрес {addr}"),
            Error::Startup(..) => write!(f, "не стартую без зависимостей"),
            Error::Serve(..) => write!(f, "gRPC-сервер упал"),
            Error::Background(..) => write!(f, "упала критичная фоновая задача"),
            Error::Tls(..) => write!(f, "TLS не настроился"),
            Error::Certs(..) => write!(f, "нет сертификатов сервиса"),
            Error::Io(..) => write!(f, "ошибка сокета"),
            Error::Bus(..) => write!(f, "NATS не отвечает"),
            Error::Db(..) => write!(f, "Postgres не готов"),
            Error::NoStorage => write!(f, "нужна база: USERS_DATABASE_URL или DATABASE_URL"),
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
            Error::Serve(error) | Error::Tls(error) => Some(error),
            Error::Io(error) | Error::Certs(error) => Some(error),
            Error::Bus(error) => Some(error),
            Error::Db(error) => Some(error),
            Error::NoStorage => None,
        }
    }
}
