use std::error::Error;
use std::fmt;

use crate::domain::DomainError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    Empty,
    UnknownCommand(String),
    MissingArgument(&'static str),
    ExtraArgument(String),
    BadNumber(String),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::Empty => write!(f, "пустая команда"),
            ParseError::UnknownCommand(name) => write!(f, "неизвестная команда: {name}"),
            ParseError::MissingArgument(what) => write!(f, "не хватает аргумента: {what}"),
            ParseError::ExtraArgument(extra) => write!(f, "лишний аргумент: {extra}"),
            ParseError::BadNumber(word) => write!(f, "не число: {word}"),
        }
    }
}

impl Error for ParseError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandError {
    Parse(ParseError),
    Domain(DomainError),
    UnknownName(String),
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
        }
    }
}

impl Error for CommandError {}
