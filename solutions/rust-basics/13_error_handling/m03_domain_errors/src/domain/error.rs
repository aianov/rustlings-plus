use std::error::Error;
use std::fmt;

use super::{MAX_NAME_LEN, MAX_POST_LEN, PostId, UserId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DomainError {
    EmptyName,
    NameTooLong,
    NameTaken(String),
    SelfFollow,
    UnknownUser(UserId),
    UnknownPost(PostId),
    EmptyPost,
    PostTooLong,
}

impl fmt::Display for DomainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DomainError::EmptyName => write!(f, "ник не может быть пустым"),
            DomainError::NameTooLong => write!(f, "ник длиннее {MAX_NAME_LEN} символов"),
            DomainError::NameTaken(name) => write!(f, "ник {name} уже занят"),
            DomainError::SelfFollow => write!(f, "нельзя подписаться на самого себя"),
            DomainError::UnknownUser(id) => write!(f, "нет пользователя с id {}", id.0),
            DomainError::UnknownPost(id) => write!(f, "нет поста с id {}", id.0),
            DomainError::EmptyPost => write!(f, "пост не может быть пустым"),
            DomainError::PostTooLong => write!(f, "пост длиннее {MAX_POST_LEN} символов"),
        }
    }
}

impl Error for DomainError {}
