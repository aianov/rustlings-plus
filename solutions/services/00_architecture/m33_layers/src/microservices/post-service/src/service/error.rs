use std::error::Error as StdError;
use std::fmt;

use contracts::{PostId, UserId};
use journal::JournalError;

use super::directory::DirectoryError;
use crate::common::{MAX_COMMENT_LEN, MAX_POST_LEN};

#[derive(Debug)]
pub enum PostError {
    EmptyPost,
    PostTooLong,
    EmptyComment,
    CommentTooLong,
    UnknownAuthor(UserId),
    UnknownUser(UserId),
    UnknownPost(PostId),
    NotAuthor(PostId),
    Directory(DirectoryError),
    Storage(JournalError),
}

impl PostError {
    pub fn code(&self) -> &'static str {
        match self {
            PostError::EmptyPost
            | PostError::PostTooLong
            | PostError::EmptyComment
            | PostError::CommentTooLong => "rejected",
            PostError::UnknownAuthor(..)
            | PostError::UnknownUser(..)
            | PostError::UnknownPost(..) => "not_found",
            PostError::NotAuthor(..) => "forbidden",
            PostError::Directory(DirectoryError::Unavailable) => "unavailable",
            PostError::Directory(DirectoryError::Failed(..)) | PostError::Storage(..) => "internal",
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
            PostError::EmptyPost => write!(f, "пост не может быть пустым"),
            PostError::PostTooLong => write!(f, "пост длиннее {MAX_POST_LEN} символов"),
            PostError::EmptyComment => write!(f, "комментарий не может быть пустым"),
            PostError::CommentTooLong => {
                write!(f, "комментарий длиннее {MAX_COMMENT_LEN} символов")
            }
            PostError::UnknownAuthor(id) => write!(f, "нет пользователя с id {}", id.0),
            PostError::UnknownUser(id) => write!(f, "нет пользователя с id {}", id.0),
            PostError::UnknownPost(id) => write!(f, "нет поста с id {}", id.0),
            PostError::NotAuthor(id) => write!(f, "удалить пост [{}] может только автор", id.0),
            PostError::Directory(error) => write!(f, "{error}"),
            PostError::Storage(..) => write!(f, "хранилище постов не записало изменение"),
        }
    }
}

impl StdError for PostError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            PostError::Storage(error) => Some(error),
            _ => None,
        }
    }
}
