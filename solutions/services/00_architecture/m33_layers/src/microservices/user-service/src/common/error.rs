use std::error::Error as StdError;
use std::fmt;
use std::io;

#[derive(Debug)]
pub enum Error {
    Journal(journal::JournalError),
    Bind { addr: String, source: io::Error },
    Serve(tonic::transport::Error),
    Tls(tonic::transport::Error),
    Certs(io::Error),
    Io(io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

impl From<journal::JournalError> for Error {
    fn from(error: journal::JournalError) -> Error {
        Error::Journal(error)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Journal(..) => write!(f, "журнал профилей не поднялся"),
            Error::Bind { addr, .. } => write!(f, "не занять адрес {addr}"),
            Error::Serve(..) => write!(f, "gRPC-сервер упал"),
            Error::Tls(..) => write!(f, "TLS не настроился"),
            Error::Certs(..) => write!(f, "нет сертификатов сервиса"),
            Error::Io(..) => write!(f, "ошибка сокета"),
        }
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Error::Journal(error) => Some(error),
            Error::Bind { source, .. } => Some(source),
            Error::Serve(error) | Error::Tls(error) => Some(error),
            Error::Io(error) | Error::Certs(error) => Some(error),
        }
    }
}
