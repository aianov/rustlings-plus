use std::error::Error as StdError;
use std::fmt;
use std::io;

#[derive(Debug)]
pub enum StorageError {
    Io(io::Error),
    Broken(String),
}

impl StorageError {
    pub fn broken(reason: impl fmt::Display) -> StorageError {
        StorageError::Broken(reason.to_string())
    }
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StorageError::Io(..) => write!(f, "хранилище не читается и не пишется"),
            StorageError::Broken(reason) => write!(f, "хранилище отказало: {reason}"),
        }
    }
}

impl StdError for StorageError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            StorageError::Io(error) => Some(error),
            StorageError::Broken(..) => None,
        }
    }
}

impl From<io::Error> for StorageError {
    fn from(error: io::Error) -> StorageError {
        StorageError::Io(error)
    }
}
