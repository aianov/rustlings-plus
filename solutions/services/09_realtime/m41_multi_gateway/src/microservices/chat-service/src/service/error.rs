use std::error::Error as StdError;
use std::fmt;

use contracts::{ErrorCode, UserId};
use journal::JournalError;

use super::directory::DirectoryError;

#[derive(Debug)]
pub enum ChatError {
    SelfMessage,
    UnknownPeer(UserId),
    Directory(DirectoryError),
    Storage(JournalError),
    Undelivered,
}

impl ChatError {
    pub fn code(&self) -> ErrorCode {
        match self {
            ChatError::SelfMessage => ErrorCode::Rejected,
            ChatError::UnknownPeer(..) => ErrorCode::NotFound,
            ChatError::Directory(DirectoryError::Unavailable) | ChatError::Undelivered => {
                ErrorCode::Unavailable
            }
            ChatError::Directory(DirectoryError::Failed(..)) | ChatError::Storage(..) => {
                ErrorCode::Internal
            }
        }
    }
}

impl From<DirectoryError> for ChatError {
    fn from(error: DirectoryError) -> ChatError {
        ChatError::Directory(error)
    }
}

impl From<JournalError> for ChatError {
    fn from(error: JournalError) -> ChatError {
        ChatError::Storage(error)
    }
}

impl fmt::Display for ChatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ChatError::SelfMessage => write!(f, "нельзя написать самому себе"),
            ChatError::UnknownPeer(id) => write!(f, "нет пользователя с id {}", id.0),
            ChatError::Directory(error) => write!(f, "{error}"),
            ChatError::Storage(..) => write!(f, "журнал сообщений не записал сообщение"),
            ChatError::Undelivered => {
                write!(f, "«печатает» не отправлено: нет связи с брокером")
            }
        }
    }
}

impl StdError for ChatError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            ChatError::Storage(error) => Some(error),
            _ => None,
        }
    }
}
