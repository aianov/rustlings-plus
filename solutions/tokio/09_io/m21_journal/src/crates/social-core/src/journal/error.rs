use std::error::Error;
use std::fmt;
use std::io;

use crate::domain::DomainError;

#[derive(Debug)]
pub enum JournalError {
    Io(io::Error),
    TooLong {
        line: usize,
        max: usize,
    },
    Bad {
        line: usize,
        reason: String,
    },
    Rejected {
        line: usize,
        error: DomainError,
    },
    Diverged {
        line: usize,
        expected: String,
        got: String,
    },
}

impl JournalError {
    pub fn line(&self) -> Option<usize> {
        match self {
            JournalError::Io(..) => None,
            JournalError::TooLong { line, .. }
            | JournalError::Bad { line, .. }
            | JournalError::Rejected { line, .. }
            | JournalError::Diverged { line, .. } => Some(*line),
        }
    }
}

impl fmt::Display for JournalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JournalError::Io(..) => write!(f, "журнал не читается"),
            JournalError::TooLong { line, max } => {
                write!(f, "строка {line} журнала длиннее {max} байт")
            }
            JournalError::Bad { line, reason } => {
                write!(f, "строка {line} журнала не разобрана: {reason}")
            }
            JournalError::Rejected { line, .. } => {
                write!(f, "строка {line} журнала не применилась")
            }
            JournalError::Diverged {
                line,
                expected,
                got,
            } => write!(
                f,
                "строка {line} журнала разошлась с ядром: в журнале {expected}, а вышло {got}"
            ),
        }
    }
}

impl Error for JournalError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            JournalError::Io(error) => Some(error),
            JournalError::Rejected { error, .. } => Some(error),
            JournalError::TooLong { .. }
            | JournalError::Bad { .. }
            | JournalError::Diverged { .. } => None,
        }
    }
}
