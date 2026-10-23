use std::error::Error as StdError;
use std::fmt;

use contracts::{NameError, UserId};
use journal::JournalError;

#[derive(Debug)]
pub enum UserError {
    BadName(NameError),
    NameTaken(String),
    UnknownUser(UserId),
    UnknownName(String),
    SelfFollow,
    SelfMute,
    NotImage(String),
    TooLarge(usize),
    NoAvatar(UserId),
    Storage(JournalError),
}

impl UserError {
    pub fn code(&self) -> &'static str {
        match self {
            UserError::BadName(..) => "bad_data",
            UserError::NameTaken(..) => "conflict",
            UserError::UnknownUser(..) | UserError::UnknownName(..) | UserError::NoAvatar(..) => {
                "not_found"
            }
            UserError::SelfFollow | UserError::SelfMute => "rejected",
            UserError::NotImage(..) => "unsupported",
            UserError::TooLarge(..) => "too_large",
            UserError::Storage(..) => "internal",
        }
    }
}

impl From<JournalError> for UserError {
    fn from(error: JournalError) -> UserError {
        UserError::Storage(error)
    }
}

impl fmt::Display for UserError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UserError::BadName(error) => write!(f, "{error}"),
            UserError::NameTaken(name) => write!(f, "ник @{name} уже занят"),
            UserError::UnknownUser(id) => write!(f, "нет пользователя с id {}", id.0),
            UserError::UnknownName(name) => write!(f, "нет пользователя @{name}"),
            UserError::SelfFollow => write!(f, "нельзя подписаться на самого себя"),
            UserError::SelfMute => write!(f, "нельзя скрыть самого себя"),
            UserError::NotImage(kind) => write!(f, "аватар это картинка image/..., а не {kind}"),
            UserError::TooLarge(limit) => write!(f, "аватар больше {limit} байт"),
            UserError::NoAvatar(id) => write!(f, "у {id} нет аватара"),
            UserError::Storage(..) => write!(f, "хранилище профилей не записало изменение"),
        }
    }
}

impl StdError for UserError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            UserError::BadName(error) => Some(error),
            UserError::Storage(error) => Some(error),
            _ => None,
        }
    }
}
