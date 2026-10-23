use std::fmt;

use serde_json::{Value, json};

use crate::dto::FieldError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiError {
    Unauthorized,
    Validation(Vec<FieldError>),
    IdempotencyConflict,
    ShuttingDown,
}

impl ApiError {
    pub fn status(&self) -> u16 {
        match self {
            ApiError::Unauthorized => 401,
            ApiError::Validation(_) => 400,
            ApiError::IdempotencyConflict => 409,
            ApiError::ShuttingDown => 503,
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            ApiError::Unauthorized => "unauthorized",
            ApiError::Validation(_) => "validation",
            ApiError::IdempotencyConflict => "idempotency_conflict",
            ApiError::ShuttingDown => "shutting_down",
        }
    }

    pub fn body(&self) -> Value {
        let mut error = json!({ "code": self.code(), "message": self.to_string() });
        if let ApiError::Validation(fields) = self {
            error["fields"] = json!(fields);
        }
        json!({ "error": error })
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ApiError::Unauthorized => "ключ API не подошёл",
            ApiError::Validation(_) => "запрос не прошёл проверку",
            ApiError::IdempotencyConflict => "ключ идемпотентности уже использован с другим событием",
            ApiError::ShuttingDown => "служба останавливается",
        })
    }
}

impl std::error::Error for ApiError {}
