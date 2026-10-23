use std::error::Error as StdError;
use std::fmt;
use std::io;

#[derive(Debug)]
pub enum Error {
    Core(social_core::Error),
    Runtime(io::Error),
    Read { line: usize, source: io::Error },
    Write(io::Error),
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
            Error::Core(..) => write!(f, "ядро не запустилось"),
            Error::Runtime(..) => write!(f, "не запустился рантайм ядра"),
            Error::Read { line, .. } => write!(f, "не удалось прочитать строку {line}"),
            Error::Write(..) => write!(f, "не удалось записать ответ"),
        }
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Error::Core(error) => Some(error),
            Error::Runtime(error) => Some(error),
            Error::Read { source, .. } => Some(source),
            Error::Write(error) => Some(error),
        }
    }
}
