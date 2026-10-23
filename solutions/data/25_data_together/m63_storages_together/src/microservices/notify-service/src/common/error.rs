use db_engine::DbError;
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
    Upstream(tonic::transport::Error),
    Startup(StartupError),
    Background(Fatal),
    Certs(io::Error),
    Io(io::Error),
    Db(DbError),
    NoStorage,
}

pub type Result<T> = std::result::Result<T, Error>;

impl From<ConfigError> for Error {
    fn from(error: ConfigError) -> Error {
        Error::Config(error)
    }
}

impl From<Halt<BusError>> for Error {
    fn from(halt: Halt<BusError>) -> Error {
        match halt {
            Halt::Startup(error) => Error::Startup(error),
            Halt::Serve(error) => Error::Bus(error),
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
            Error::Upstream(..) => write!(f, "канал к user-service не настроился"),
            Error::Startup(..) => write!(f, "не стартую без зависимостей"),
            Error::Background(..) => write!(f, "упала критичная фоновая задача"),
            Error::Certs(..) => write!(f, "нет сертификатов сервиса"),
            Error::Io(..) => write!(f, "ошибка сокета"),
            Error::Db(error) => write!(f, "Postgres уведомлений: {error}"),
            Error::NoStorage => write!(f, "нужна база: NOTIFY_DATABASE_URL или DATABASE_URL"),
        }
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Error::Config(error) => Some(error),
            Error::Bind { source, .. } => Some(source),
            Error::Bus(error) => Some(error),
            Error::Upstream(error) => Some(error),
            Error::Startup(error) => Some(error),
            Error::Background(error) => Some(error),
            Error::Io(error) | Error::Certs(error) => Some(error),
            Error::Db(error) => Some(error),
            Error::NoStorage => None,
        }
    }
}
