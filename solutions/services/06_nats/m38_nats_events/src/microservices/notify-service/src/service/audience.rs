use std::error::Error;
use std::fmt;

use contracts::UserId;

use crate::repository::BoxFuture;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudienceError {
    Unavailable,
    Failed(String),
}

impl fmt::Display for AudienceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AudienceError::Unavailable => write!(f, "user-service недоступен"),
            AudienceError::Failed(reason) => write!(f, "user-service ответил ошибкой: {reason}"),
        }
    }
}

impl Error for AudienceError {}

pub trait Audience: Send + Sync {
    fn readers(
        &self,
        author: UserId,
        request_id: &str,
    ) -> BoxFuture<'_, Result<Vec<UserId>, AudienceError>>;
}
