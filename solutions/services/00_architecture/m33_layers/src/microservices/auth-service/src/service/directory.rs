use std::fmt;

use crate::common::Account;
use crate::repository::BoxFuture;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileError {
    Refused { code: String, message: String },
    Unavailable,
    Failed(String),
}

impl fmt::Display for ProfileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProfileError::Refused { message, .. } => write!(f, "{message}"),
            ProfileError::Unavailable => write!(f, "сервис профилей недоступен, повтори позже"),
            ProfileError::Failed(why) => write!(f, "сервис профилей ответил ошибкой: {why}"),
        }
    }
}

pub trait ProfileDirectory: Send + Sync {
    fn create(&self, username: String) -> BoxFuture<'_, Result<Account, ProfileError>>;
}
