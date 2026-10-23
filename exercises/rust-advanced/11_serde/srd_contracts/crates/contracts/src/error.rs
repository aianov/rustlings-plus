use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq)]
pub enum ContractError {
    Truncated,
    Syntax { line: usize, column: usize },
    BadEnvelope { reason: String },
    UnknownMethod(String),
    BadData { method: String, reason: String },
}

#[derive(Debug, PartialEq, Clone)]
pub struct ErrorBody {
    pub code: String,
    pub message: String,
}

impl ContractError {
    pub fn to_body(&self) -> ErrorBody {
        todo!()
    }
}
