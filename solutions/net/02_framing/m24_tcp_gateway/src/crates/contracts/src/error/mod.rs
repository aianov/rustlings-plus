use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::error::Category;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Truncated,
    Syntax { line: usize, column: usize },
    BadEnvelope { reason: String },
    UnknownMethod(String),
    WrongService { service: String, method: String },
    BadData { method: String, reason: String },
    Forbidden { method: String, claimed: String },
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorBody {
    pub code: String,
    pub message: String,
}

impl Error {
    pub(crate) fn from_json(error: &serde_json::Error) -> Error {
        match error.classify() {
            Category::Eof => Error::Truncated,
            Category::Syntax | Category::Io => Error::Syntax {
                line: error.line(),
                column: error.column(),
            },
            Category::Data => Error::BadEnvelope {
                reason: reason_of(error),
            },
        }
    }

    pub(crate) fn bad_data(method: &str, reason: impl fmt::Display) -> Error {
        Error::BadData {
            method: method.to_string(),
            reason: reason.to_string(),
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Error::Truncated => "truncated",
            Error::Syntax { .. } => "syntax",
            Error::BadEnvelope { .. } => "bad_envelope",
            Error::UnknownMethod(..) => "unknown_method",
            Error::WrongService { .. } => "wrong_service",
            Error::BadData { .. } => "bad_data",
            Error::Forbidden { .. } => "forbidden",
        }
    }
}

fn reason_of(error: &serde_json::Error) -> String {
    let text = error.to_string();
    match text.rsplit_once(" at line ") {
        Some((reason, _)) => reason.to_string(),
        None => text,
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Truncated => write!(f, "конверт оборван"),
            Error::Syntax { line, column } => {
                write!(f, "не JSON (строка {line}, столбец {column})")
            }
            Error::BadEnvelope { reason } => write!(f, "кривой конверт: {reason}"),
            Error::UnknownMethod(method) => write!(f, "неизвестный метод {method}"),
            Error::WrongService { service, method } => {
                write!(f, "метод {method} не из сервиса {service}")
            }
            Error::BadData { method, reason } => write!(f, "плохие данные для {method}: {reason}"),
            Error::Forbidden { method, claimed } => {
                write!(
                    f,
                    "{method}: соединение действует только от своего имени, а в data user {claimed}"
                )
            }
        }
    }
}

impl std::error::Error for Error {}
