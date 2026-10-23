use std::error::Error as StdError;
use std::fmt;
use std::io;

#[derive(Debug)]
pub enum JournalError {
    Io(io::Error),
    Broken { line: usize, reason: String },
    Encode(serde_json::Error),
}

impl fmt::Display for JournalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JournalError::Io(..) => write!(f, "журнал не читается и не пишется"),
            JournalError::Broken { line, reason } => {
                write!(f, "журнал испорчен в строке {line}: {reason}")
            }
            JournalError::Encode(..) => write!(f, "запись журнала не кодируется"),
        }
    }
}

impl StdError for JournalError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            JournalError::Io(error) => Some(error),
            JournalError::Broken { .. } => None,
            JournalError::Encode(error) => Some(error),
        }
    }
}

impl From<io::Error> for JournalError {
    fn from(error: io::Error) -> JournalError {
        JournalError::Io(error)
    }
}
