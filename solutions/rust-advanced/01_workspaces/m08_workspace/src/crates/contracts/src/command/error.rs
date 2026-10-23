use std::error::Error;
use std::fmt;

use crate::username::NameError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    Empty,
    UnknownCommand(String),
    MissingArgument(&'static str),
    ExtraArgument(String),
    BadNumber(String),
    BadName(NameError),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::Empty => write!(f, "пустая команда"),
            ParseError::UnknownCommand(name) => write!(f, "неизвестная команда: {name}"),
            ParseError::MissingArgument(what) => write!(f, "не хватает аргумента: {what}"),
            ParseError::ExtraArgument(extra) => write!(f, "лишний аргумент: {extra}"),
            ParseError::BadNumber(word) => write!(f, "не число: {word}"),
            ParseError::BadName(error) => write!(f, "{error}"),
        }
    }
}

impl Error for ParseError {}
