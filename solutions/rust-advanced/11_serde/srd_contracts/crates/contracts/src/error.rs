use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, PartialEq)]
pub enum ContractError {
    Truncated,
    Syntax { line: usize, column: usize },
    BadEnvelope { reason: String },
    UnknownMethod(String),
    BadData { method: String, reason: String },
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct ErrorBody {
    pub code: String,
    pub message: String,
}

pub(crate) fn reason_of(error: &serde_json::Error) -> String {
    let text = error.to_string();
    text.rsplit_once(" at line ").map_or(text.as_str(), |(head, _)| head).to_string()
}

impl ContractError {
    pub(crate) fn from_json(error: &serde_json::Error) -> ContractError {
        use serde_json::error::Category;

        match error.classify() {
            Category::Eof => ContractError::Truncated,
            Category::Syntax | Category::Io => ContractError::Syntax { line: error.line(), column: error.column() },
            Category::Data => ContractError::BadEnvelope { reason: reason_of(error) },
        }
    }

    pub fn to_body(&self) -> ErrorBody {
        let code = match self {
            ContractError::Truncated => "truncated",
            ContractError::Syntax { .. } => "syntax",
            ContractError::BadEnvelope { .. } => "bad_envelope",
            ContractError::UnknownMethod(_) => "unknown_method",
            ContractError::BadData { .. } => "bad_data",
        };
        ErrorBody { code: code.to_string(), message: self.to_string() }
    }
}

impl fmt::Display for ContractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ContractError::Truncated => write!(f, "message is incomplete"),
            ContractError::Syntax { line, column } => write!(f, "invalid JSON at line {line} column {column}"),
            ContractError::BadEnvelope { reason } => write!(f, "bad envelope: {reason}"),
            ContractError::UnknownMethod(method) => write!(f, "unknown method {method}"),
            ContractError::BadData { method, reason } => write!(f, "bad data for {method}: {reason}"),
        }
    }
}

impl std::error::Error for ContractError {}
