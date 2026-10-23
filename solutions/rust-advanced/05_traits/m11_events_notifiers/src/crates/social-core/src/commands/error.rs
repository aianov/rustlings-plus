use std::error::Error;
use std::fmt;

use contracts::ParseError;

use crate::domain::DomainError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandError {
    Parse(ParseError),
    Domain(DomainError),
    UnknownName(String),
    NoExtension(&'static str),
}

impl From<ParseError> for CommandError {
    fn from(error: ParseError) -> CommandError {
        CommandError::Parse(error)
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
            CommandError::NoExtension(name) => write!(f, "в контексте нет {name}"),
        }
    }
}

impl Error for CommandError {}
