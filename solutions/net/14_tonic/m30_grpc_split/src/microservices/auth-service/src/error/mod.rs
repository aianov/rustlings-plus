use std::error::Error as StdError;
use std::fmt;
use std::io;

#[derive(Debug)]
pub enum Error {
    Journal(io::Error),
    Bind { addr: String, source: io::Error },
    Serve(tonic::transport::Error),
    Upstream(tonic::transport::Error),
    Io(io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Journal(..) => write!(f, "журнал учётных записей не читается"),
            Error::Bind { addr, .. } => write!(f, "не занять адрес {addr}"),
            Error::Serve(..) => write!(f, "gRPC-сервер упал"),
            Error::Upstream(..) => write!(f, "плохой адрес user-service"),
            Error::Io(..) => write!(f, "ошибка сокета"),
        }
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Error::Journal(error) | Error::Io(error) => Some(error),
            Error::Bind { source, .. } => Some(source),
            Error::Serve(error) | Error::Upstream(error) => Some(error),
        }
    }
}
