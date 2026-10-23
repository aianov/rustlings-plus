use std::error::Error as StdError;
use std::fmt;

use contracts::{ErrorBody, ErrorCode, PostId, UserId};
use journal::JournalError;
use validation::{Invalid, Violation};

use super::directory::DirectoryError;

#[derive(Debug)]
pub enum PostError {
    UnknownAuthor(UserId),
    UnknownUser(UserId),
    UnknownPost(PostId),
    NotAuthor(PostId),
    Directory(DirectoryError),
    Storage(JournalError),
}

impl PostError {
    pub fn code(&self) -> ErrorCode {
        match self {
            PostError::UnknownAuthor(..) => ErrorCode::Validation,
            PostError::UnknownUser(..) | PostError::UnknownPost(..) => ErrorCode::NotFound,
            PostError::NotAuthor(..) => ErrorCode::Forbidden,
            PostError::Directory(DirectoryError::Unavailable) => ErrorCode::Unavailable,
            PostError::Directory(DirectoryError::Failed(..)) | PostError::Storage(..) => {
                ErrorCode::Internal
            }
        }
    }

    pub fn body(&self) -> ErrorBody {
        match self {
            PostError::UnknownAuthor(..) => {
                let unknown = Violation::new("unknown", self.to_string());
                ErrorBody::invalid(&Invalid::single("author", unknown))
            }
            _ => ErrorBody::new(self.code(), self.to_string()),
        }
    }
}

impl From<JournalError> for PostError {
    fn from(error: JournalError) -> PostError {
        PostError::Storage(error)
    }
}

impl From<DirectoryError> for PostError {
    fn from(error: DirectoryError) -> PostError {
        PostError::Directory(error)
    }
}

impl fmt::Display for PostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PostError::UnknownAuthor(id) => {
                write!(f, "автора с id {} нет в user-service", id.0)
            }
            PostError::UnknownUser(id) => write!(f, "нет пользователя с id {}", id.0),
            PostError::UnknownPost(id) => write!(f, "нет поста с id {}", id.0),
            PostError::NotAuthor(id) => {
                write!(
                    f,
                    "пост [{}] меняет только автор, а удаляет ещё модератор",
                    id.0
                )
            }
            PostError::Directory(error) => write!(f, "{error}"),
            PostError::Storage(..) => write!(f, "хранилище постов не записало изменение"),
        }
    }
}

impl StdError for PostError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            PostError::Storage(error) => Some(error),
            PostError::Directory(error) => Some(error),
            _ => None,
        }
    }
}
