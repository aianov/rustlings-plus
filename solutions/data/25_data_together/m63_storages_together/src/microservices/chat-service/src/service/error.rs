use std::error::Error as StdError;
use std::fmt;

use contracts::{ErrorCode, UserId};
use service_runtime::StorageError;
use store_engine::StoreError;

use super::directory::DirectoryError;

#[derive(Debug)]
pub enum ChatError {
    SelfMessage,
    UnknownPeer(UserId),
    Directory(DirectoryError),
    Storage(StorageError),
    Undelivered,
    NoPages,
    UnknownMessage(String),
    NotAuthor(String),
    BadTtl(u32),
    Store(StoreError),
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
            ChatError::Store(StoreError::BadCursor) => ErrorCode::Validation,
            ChatError::NoPages | ChatError::Store(..) => ErrorCode::Unavailable,
            ChatError::UnknownMessage(..) => ErrorCode::NotFound,
            ChatError::NotAuthor(..) => ErrorCode::Forbidden,
            ChatError::BadTtl(..) => ErrorCode::Validation,
        }
    }
}

impl From<DirectoryError> for ChatError {
    fn from(error: DirectoryError) -> ChatError {
        ChatError::Directory(error)
    }
}

impl From<StoreError> for ChatError {
    fn from(error: StoreError) -> ChatError {
        ChatError::Store(error)
    }
}

impl From<StorageError> for ChatError {
    fn from(error: StorageError) -> ChatError {
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
            ChatError::NoPages => write!(f, "страницы переписки и список чатов живут в Scylla"),
            ChatError::UnknownMessage(id) => write!(f, "нет сообщения {id}"),
            ChatError::NotAuthor(id) => write!(f, "сообщение {id} правит только автор"),
            ChatError::BadTtl(ttl) => write!(
                f,
                "срок жизни сообщения 1..={} секунд, а не {ttl}",
                super::MAX_TTL
            ),
            ChatError::Store(error) => write!(f, "{error}"),
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
