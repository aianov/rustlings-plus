use std::error::Error as StdError;
use std::fmt;
use std::io;

use service_runtime::{ConfigError, Fatal, Halt, StartupError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Duplicates(pub Vec<String>);

impl fmt::Display for Duplicates {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "маршруты заведены дважды: {}", self.0.join(", "))
    }
}

impl StdError for Duplicates {}

#[derive(Debug)]
pub enum Error {
    Routes(Duplicates),
    Upstream {
        service: &'static str,
        source: tonic::transport::Error,
    },
    Bind {
        addr: String,
        source: io::Error,
    },
    Config(ConfigError),
    Startup(StartupError),
    Background(Fatal),
    Certs(io::Error),
    Io(io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

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
            Error::Routes(..) => write!(f, "таблица маршрутов не собралась"),
            Error::Upstream { service, .. } => write!(f, "плохой адрес {service}"),
            Error::Bind { addr, .. } => write!(f, "не занять адрес {addr}"),
            Error::Config(..) => write!(f, "не запускаюсь"),
            Error::Startup(..) => write!(f, "не стартую без сервисов"),
            Error::Background(..) => write!(f, "упала критичная фоновая задача"),
            Error::Certs(..) => write!(f, "нет сертификатов шлюза"),
            Error::Io(..) => write!(f, "ошибка сокета"),
        }
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Error::Routes(error) => Some(error),
            Error::Upstream { source, .. } => Some(source),
            Error::Bind { source, .. } => Some(source),
            Error::Config(error) => Some(error),
            Error::Startup(error) => Some(error),
            Error::Background(error) => Some(error),
            Error::Certs(error) | Error::Io(error) => Some(error),
        }
    }
}

pub fn report(error: &(dyn StdError + 'static)) -> String {
    let mut text = error.to_string();
    let mut cause = error.source();
    while let Some(next) = cause {
        text.push_str(": ");
        text.push_str(&next.to_string());
        cause = next.source();
    }
    text
}
