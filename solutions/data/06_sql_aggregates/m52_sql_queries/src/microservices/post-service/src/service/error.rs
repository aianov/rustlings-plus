use std::error::Error as StdError;
use std::fmt;

use contracts::{CommentId, ErrorBody, ErrorCode, PostId, UserId};
use journal::JournalError;
use validation::{Invalid, Violation};

use super::directory::DirectoryError;
use super::queries::{MAX_DAYS, MAX_IDS};

#[derive(Debug)]
pub enum PostError {
    UnknownAuthor(UserId),
    UnknownUser(UserId),
    UnknownPost(PostId),
    NotAuthor(PostId),
    UnknownComment(CommentId),
    NotCommentOwner(CommentId),
    UnknownDraft(u64),
    NoDrafts,
    NoQueries,
    NotAdmin,
    TooMany(usize),
    BadDays(u32),
    Directory(DirectoryError),
    Storage(JournalError),
}

impl PostError {
    pub fn code(&self) -> ErrorCode {
        match self {
            PostError::UnknownAuthor(..) => ErrorCode::Validation,
            PostError::UnknownUser(..) | PostError::UnknownPost(..) => ErrorCode::NotFound,
            PostError::NotAuthor(..) | PostError::NotCommentOwner(..) => ErrorCode::Forbidden,
            PostError::UnknownComment(..) | PostError::UnknownDraft(..) => ErrorCode::NotFound,
            PostError::NoDrafts | PostError::NoQueries => ErrorCode::Unavailable,
            PostError::TooMany(..) | PostError::BadDays(..) => ErrorCode::Validation,
            PostError::NotAdmin => ErrorCode::Forbidden,
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
            PostError::UnknownComment(id) => write!(f, "нет комментария {id}"),
            PostError::NotCommentOwner(id) => {
                write!(f, "комментарий {id} удаляет его автор или автор поста")
            }
            PostError::UnknownDraft(id) => write!(f, "нет черновика {id}"),
            PostError::NoDrafts => write!(f, "черновики выключены"),
            PostError::NoQueries => write!(f, "запросы по постам выключены"),
            PostError::NotAdmin => write!(f, "статистику по дням смотрит только админ"),
            PostError::TooMany(count) => {
                write!(
                    f,
                    "за раз гидрируется не больше {MAX_IDS} постов, а пришло {count}"
                )
            }
            PostError::BadDays(days) => {
                write!(
                    f,
                    "статистика берётся за 1..={MAX_DAYS} дней, а не за {days}"
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
