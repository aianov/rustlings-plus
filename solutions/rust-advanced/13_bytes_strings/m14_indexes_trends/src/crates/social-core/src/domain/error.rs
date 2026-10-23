use std::error::Error;
use std::fmt;

use contracts::NameError;

use crate::chat::MAX_MESSAGE_LEN;

use crate::comments::MAX_COMMENT_LEN;

use super::{CommentId, MAX_NAME_LEN, MAX_POST_LEN, PostId, UserId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DomainError {
    EmptyName,
    NameTooLong,
    BadNameChar(char),
    NameTaken(String),
    SelfFollow,
    UnknownUser(UserId),
    UnknownPost(PostId),
    EmptyPost,
    PostTooLong,
    SelfMessage,
    EmptyMessage,
    MessageTooLong,
    SelfMute,
    NotAuthor(PostId),
    UnknownComment(CommentId),
    EmptyComment,
    CommentTooLong,
}

impl fmt::Display for DomainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DomainError::EmptyName => write!(f, "ник не может быть пустым"),
            DomainError::NameTooLong => write!(f, "ник длиннее {MAX_NAME_LEN} символов"),
            DomainError::BadNameChar(c) => write!(f, "{}", NameError::BadChar(*c)),
            DomainError::NameTaken(name) => write!(f, "ник {name} уже занят"),
            DomainError::SelfFollow => write!(f, "нельзя подписаться на самого себя"),
            DomainError::UnknownUser(id) => write!(f, "нет пользователя с id {}", id.0),
            DomainError::UnknownPost(id) => write!(f, "нет поста с id {}", id.0),
            DomainError::EmptyPost => write!(f, "пост не может быть пустым"),
            DomainError::PostTooLong => write!(f, "пост длиннее {MAX_POST_LEN} символов"),
            DomainError::SelfMessage => write!(f, "нельзя написать самому себе"),
            DomainError::EmptyMessage => write!(f, "сообщение не может быть пустым"),
            DomainError::MessageTooLong => {
                write!(f, "сообщение длиннее {MAX_MESSAGE_LEN} символов")
            }
            DomainError::SelfMute => write!(f, "нельзя скрыть самого себя"),
            DomainError::NotAuthor(id) => write!(f, "удалить пост [{}] может только автор", id.0),
            DomainError::UnknownComment(id) => write!(f, "нет комментария с id {}", id.0),
            DomainError::EmptyComment => write!(f, "комментарий не может быть пустым"),
            DomainError::CommentTooLong => {
                write!(f, "комментарий длиннее {MAX_COMMENT_LEN} символов")
            }
        }
    }
}

impl From<NameError> for DomainError {
    fn from(error: NameError) -> DomainError {
        match error {
            NameError::Empty => DomainError::EmptyName,
            NameError::TooLong => DomainError::NameTooLong,
            NameError::BadChar(c) => DomainError::BadNameChar(c),
        }
    }
}

impl Error for DomainError {}
