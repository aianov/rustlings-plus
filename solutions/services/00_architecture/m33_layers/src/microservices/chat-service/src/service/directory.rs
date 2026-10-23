use std::fmt;

use contracts::UserId;

use crate::repository::BoxFuture;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DirectoryError {
    Unavailable,
    Failed(String),
}

impl fmt::Display for DirectoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DirectoryError::Unavailable => {
                write!(f, "сервис пользователей недоступен, повтори позже")
            }
            DirectoryError::Failed(why) => write!(f, "сервис пользователей ответил ошибкой: {why}"),
        }
    }
}

pub trait UserDirectory: Send + Sync {
    fn exists(&self, id: UserId) -> BoxFuture<'_, Result<bool, DirectoryError>>;
}
