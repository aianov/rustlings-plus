use std::error::Error as StdError;
use std::fmt;
use std::io;

use contracts::ErrorBody;

use crate::app::{AppError, ImportError};
use crate::domain::DomainError;

#[derive(Debug)]
pub enum Error {
    Contract(contracts::Error),
    Domain(DomainError),
    UnknownName(String),
    Start(io::Error),
    NoRuntime,
    App(AppError),
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub fn code(&self) -> &'static str {
        match self {
            Error::Contract(error) => error.code(),
            Error::Domain(
                DomainError::UnknownUser(..)
                | DomainError::UnknownPost(..)
                | DomainError::UnknownComment(..),
            )
            | Error::UnknownName(..) => "not_found",
            Error::Domain(DomainError::NameTaken(..)) => "conflict",
            Error::Domain(..) => "rejected",
            Error::Start(..) | Error::NoRuntime => "internal",
            Error::App(AppError::Domain(..) | AppError::Import(..)) => "rejected",
            Error::App(..) => "unavailable",
        }
    }

    pub fn to_body(&self) -> ErrorBody {
        ErrorBody {
            code: self.code().to_string(),
            message: report(self),
        }
    }
}

impl From<contracts::Error> for Error {
    fn from(error: contracts::Error) -> Error {
        Error::Contract(error)
    }
}

impl From<AppError> for Error {
    fn from(error: AppError) -> Error {
        match error {
            AppError::Domain(error) => Error::Domain(error),
            other => Error::App(other),
        }
    }
}

impl From<ImportError> for Error {
    fn from(error: ImportError) -> Error {
        Error::App(AppError::Import(error))
    }
}

impl From<DomainError> for Error {
    fn from(error: DomainError) -> Error {
        Error::Domain(error)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Contract(..) => write!(f, "запрос не разобран"),
            Error::Domain(..) => write!(f, "запрос отклонён"),
            Error::UnknownName(name) => write!(f, "нет пользователя @{name}"),
            Error::Start(..) => write!(f, "не запустился поток уведомлений"),
            Error::NoRuntime => write!(f, "ядро запускается только внутри рантайма tokio"),
            Error::App(..) => write!(f, "ядро не справилось"),
        }
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Error::Contract(error) => Some(error),
            Error::Domain(error) => Some(error),
            Error::UnknownName(..) => None,
            Error::Start(error) => Some(error),
            Error::NoRuntime => None,
            Error::App(error) => Some(error),
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
