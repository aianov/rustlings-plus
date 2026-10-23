use std::error::Error as StdError;
use std::fmt;

use contracts::{ErrorCode, UserId};
use journal::JournalError;

#[derive(Debug)]
pub enum UserError {
    NameTaken(String),
    UnknownUser(UserId),
    UnknownName(String),
    SelfFollow,
    SelfMute,
    NotImage(String),
    TooLarge(usize),
    NoAvatar(UserId),
    Storage(JournalError),
    NoLifecycle,
    NoQueries,
    NoFiles,
    NoFile(String),
    Files(String),
}

impl UserError {
    pub fn code(&self) -> ErrorCode {
        match self {
            UserError::NameTaken(..) => ErrorCode::Conflict,
            UserError::UnknownUser(..)
            | UserError::UnknownName(..)
            | UserError::NoAvatar(..)
            | UserError::NoFile(..) => ErrorCode::NotFound,
            UserError::SelfFollow | UserError::SelfMute => ErrorCode::Rejected,
            UserError::NotImage(..) => ErrorCode::Unsupported,
            UserError::TooLarge(..) => ErrorCode::TooLarge,
            UserError::Storage(..)
            | UserError::NoLifecycle
            | UserError::NoQueries
            | UserError::NoFiles
            | UserError::Files(..) => ErrorCode::Internal,
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
            UserError::NameTaken(name) => write!(f, "ник @{name} уже занят"),
            UserError::UnknownUser(id) => write!(f, "нет пользователя с id {}", id.0),
            UserError::UnknownName(name) => write!(f, "нет пользователя @{name}"),
            UserError::SelfFollow => write!(f, "нельзя подписаться на самого себя"),
            UserError::SelfMute => write!(f, "нельзя скрыть самого себя"),
            UserError::NotImage(why) => write!(f, "аватар не принят: {why}"),
            UserError::TooLarge(limit) => write!(f, "аватар больше {limit} байт"),
            UserError::NoAvatar(id) => write!(f, "у {id} нет аватара"),
            UserError::Storage(..) => write!(f, "хранилище профилей не записало изменение"),
            UserError::NoLifecycle => write!(f, "скрытие и удаление профилей не подключены"),
            UserError::NoQueries => write!(f, "запросы по связям не подключены"),
            UserError::NoFiles => write!(f, "хранилище файлов не подключено"),
            UserError::NoFile(key) => write!(f, "файла {key} нет"),
            UserError::Files(why) => write!(f, "хранилище файлов: {why}"),
        }
    }
}

impl StdError for UserError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            UserError::Storage(error) => Some(error),
            _ => None,
        }
    }
}
