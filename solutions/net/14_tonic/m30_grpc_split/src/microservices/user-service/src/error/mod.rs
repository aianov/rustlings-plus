use std::error::Error as StdError;
use std::fmt;
use std::io;

#[derive(Debug)]
pub enum Error {
    Core(social_core::Error),
    Bind { addr: String, source: io::Error },
    Serve(tonic::transport::Error),
    Io(io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

impl From<social_core::Error> for Error {
    fn from(error: social_core::Error) -> Error {
        Error::Core(error)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Core(..) => write!(f, "ядро профилей не поднялось"),
            Error::Bind { addr, .. } => write!(f, "не занять адрес {addr}"),
            Error::Serve(..) => write!(f, "gRPC-сервер упал"),
            Error::Io(..) => write!(f, "ошибка сокета"),
        }
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Error::Core(error) => Some(error),
            Error::Bind { source, .. } => Some(source),
            Error::Serve(error) => Some(error),
            Error::Io(error) => Some(error),
        }
    }
}
