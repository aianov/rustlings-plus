use std::error::Error;
use std::fmt;

use contracts::ParseError;

use crate::app::AppError;
use crate::domain::DomainError;
use crate::error::report;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandError {
    Parse(ParseError),
    Domain(DomainError),
    UnknownName(String),
    NoRoute(&'static str),
    App(AppError),
}

impl From<ParseError> for CommandError {
    fn from(error: ParseError) -> CommandError {
        CommandError::Parse(error)
    }
}

impl From<AppError> for CommandError {
    fn from(error: AppError) -> CommandError {
        match error {
            AppError::Domain(error) => CommandError::Domain(error),
            other => CommandError::App(other),
        }
    }
}

impl From<DomainError> for CommandError {
    fn from(error: DomainError) -> CommandError {
        CommandError::Domain(error)
    }
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CommandError::Parse(error) => write!(f, "{error}"),
            CommandError::Domain(error) => write!(f, "{error}"),
            CommandError::UnknownName(name) => write!(f, "нет пользователя @{name}"),
            CommandError::NoRoute(method) => write!(f, "у метода {method} нет обработчика"),
            CommandError::App(error) => write!(f, "{}", report(error)),
        }
    }
}

impl Error for CommandError {}
