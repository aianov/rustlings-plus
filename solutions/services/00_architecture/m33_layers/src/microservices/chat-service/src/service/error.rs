use std::error::Error as StdError;
use std::fmt;

use contracts::UserId;
use journal::JournalError;

use super::directory::DirectoryError;
use crate::common::MAX_MESSAGE_LEN;

#[derive(Debug)]
pub enum ChatError {
    SelfMessage,
    EmptyMessage,
    MessageTooLong,
    UnknownPeer(UserId),
    Directory(DirectoryError),
    Storage(JournalError),
    Closing,
}

impl ChatError {
    pub fn code(&self) -> &'static str {
        match self {
            ChatError::SelfMessage | ChatError::EmptyMessage | ChatError::MessageTooLong => {
                "rejected"
            }
            ChatError::UnknownPeer(..) => "not_found",
            ChatError::Directory(DirectoryError::Unavailable) | ChatError::Closing => "unavailable",
            ChatError::Directory(DirectoryError::Failed(..)) | ChatError::Storage(..) => "internal",
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
            ChatError::EmptyMessage => write!(f, "сообщение не может быть пустым"),
            ChatError::MessageTooLong => {
                write!(f, "сообщение длиннее {MAX_MESSAGE_LEN} символов")
            }
            ChatError::UnknownPeer(id) => write!(f, "нет пользователя с id {}", id.0),
            ChatError::Directory(error) => write!(f, "{error}"),
            ChatError::Storage(..) => write!(f, "журнал сообщений не записал сообщение"),
            ChatError::Closing => write!(f, "chat-service останавливается"),
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
