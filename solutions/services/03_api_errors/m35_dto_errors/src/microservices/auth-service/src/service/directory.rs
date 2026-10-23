use std::fmt;

use contracts::ErrorBody;

use crate::common::Account;
use crate::repository::BoxFuture;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileError {
    Refused(ErrorBody),
    Unavailable,
    Failed(String),
}

impl fmt::Display for ProfileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProfileError::Refused(body) => write!(f, "{}", body.message),
            ProfileError::Unavailable => write!(f, "сервис профилей недоступен, повтори позже"),
            ProfileError::Failed(why) => write!(f, "сервис профилей ответил ошибкой: {why}"),
        }
    }
}

impl std::error::Error for ProfileError {}

pub trait ProfileDirectory: Send + Sync {
    fn create(&self, username: String) -> BoxFuture<'_, Result<Account, ProfileError>>;
}
