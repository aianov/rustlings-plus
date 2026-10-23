use std::error::Error as StdError;
use std::fmt;
use std::io;

#[derive(Debug)]
pub enum Error {
    Upstream {
        service: &'static str,
        source: tonic::transport::Error,
    },
    Bind {
        addr: String,
        source: io::Error,
    },
    Io(io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Upstream { service, .. } => write!(f, "плохой адрес {service}"),
            Error::Bind { addr, .. } => write!(f, "не занять адрес {addr}"),
            Error::Io(..) => write!(f, "ошибка сокета"),
        }
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Error::Upstream { source, .. } => Some(source),
            Error::Bind { source, .. } => Some(source),
            Error::Io(error) => Some(error),
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
