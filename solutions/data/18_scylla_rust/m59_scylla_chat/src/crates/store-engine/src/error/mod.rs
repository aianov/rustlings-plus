use std::fmt;

use scylla::transport::errors::{DbError, NewSessionError, QueryError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreError {
    BadKeyspace(String),
    BadCursor,
    Invalid(String),
    Unavailable(String),
    Timeout,
    Schema(String),
    Other(String),
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoreError::BadKeyspace(name) => write!(f, "«{name}» не имя keyspace"),
            StoreError::BadCursor => write!(f, "курсор не из прошлого ответа"),
            StoreError::Invalid(why) => write!(f, "Scylla отклонила запрос: {why}"),
            StoreError::Unavailable(why) => write!(f, "Scylla недоступна: {why}"),
            StoreError::Timeout => write!(f, "Scylla не ответила в срок"),
            StoreError::Schema(why) => write!(f, "схема не применилась: {why}"),
            StoreError::Other(why) => write!(f, "Scylla: {why}"),
        }
    }
}

impl std::error::Error for StoreError {}

impl From<QueryError> for StoreError {
    fn from(error: QueryError) -> StoreError {
        match error {
            QueryError::DbError(DbError::Invalid | DbError::SyntaxError, why) => {
                StoreError::Invalid(why)
            }
            QueryError::DbError(
                DbError::Unavailable { .. } | DbError::Overloaded | DbError::IsBootstrapping,
                why,
            ) => StoreError::Unavailable(why),
            QueryError::IoError(error) => StoreError::Unavailable(error.to_string()),
            QueryError::DbError(DbError::ReadTimeout { .. } | DbError::WriteTimeout { .. }, _)
            | QueryError::TimeoutError
            | QueryError::RequestTimeout(_) => StoreError::Timeout,
            other => StoreError::Other(other.to_string()),
        }
    }
}

impl From<NewSessionError> for StoreError {
    fn from(error: NewSessionError) -> StoreError {
        StoreError::Unavailable(error.to_string())
    }
}
