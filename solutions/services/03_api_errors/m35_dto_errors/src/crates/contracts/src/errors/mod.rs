mod json;

use serde::{Deserialize, Serialize};
use validation::Invalid;

pub use json::{json_error, parse_body};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorCode {
    Syntax,
    Truncated,
    BadEnvelope,
    UnknownMethod,
    WrongService,
    BadData,
    Validation,
    Unauthorized,
    Forbidden,
    NotFound,
    Conflict,
    TooLarge,
    Unsupported,
    Rejected,
    Unavailable,
    Timeout,
    Internal,
}

impl ErrorCode {
    pub const ALL: [ErrorCode; 17] = [
        ErrorCode::Syntax,
        ErrorCode::Truncated,
        ErrorCode::BadEnvelope,
        ErrorCode::UnknownMethod,
        ErrorCode::WrongService,
        ErrorCode::BadData,
        ErrorCode::Validation,
        ErrorCode::Unauthorized,
        ErrorCode::Forbidden,
        ErrorCode::NotFound,
        ErrorCode::Conflict,
        ErrorCode::TooLarge,
        ErrorCode::Unsupported,
        ErrorCode::Rejected,
        ErrorCode::Unavailable,
        ErrorCode::Timeout,
        ErrorCode::Internal,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            ErrorCode::Syntax => "syntax",
            ErrorCode::Truncated => "truncated",
            ErrorCode::BadEnvelope => "bad_envelope",
            ErrorCode::UnknownMethod => "unknown_method",
            ErrorCode::WrongService => "wrong_service",
            ErrorCode::BadData => "bad_data",
            ErrorCode::Validation => "validation",
            ErrorCode::Unauthorized => "unauthorized",
            ErrorCode::Forbidden => "forbidden",
            ErrorCode::NotFound => "not_found",
            ErrorCode::Conflict => "conflict",
            ErrorCode::TooLarge => "too_large",
            ErrorCode::Unsupported => "unsupported",
            ErrorCode::Rejected => "rejected",
            ErrorCode::Unavailable => "unavailable",
            ErrorCode::Timeout => "timeout",
            ErrorCode::Internal => "internal",
        }
    }

    pub fn from_wire(text: &str) -> Option<ErrorCode> {
        ErrorCode::ALL
            .into_iter()
            .find(|code| code.as_str() == text)
    }

    pub fn http_status(self) -> u16 {
        match self {
            ErrorCode::Syntax
            | ErrorCode::Truncated
            | ErrorCode::BadEnvelope
            | ErrorCode::UnknownMethod
            | ErrorCode::WrongService
            | ErrorCode::BadData => 400,
            ErrorCode::Unauthorized => 401,
            ErrorCode::Forbidden => 403,
            ErrorCode::NotFound => 404,
            ErrorCode::Conflict => 409,
            ErrorCode::TooLarge => 413,
            ErrorCode::Unsupported => 415,
            ErrorCode::Validation | ErrorCode::Rejected => 422,
            ErrorCode::Unavailable => 503,
            ErrorCode::Timeout => 504,
            ErrorCode::Internal => 500,
        }
    }

    pub fn for_status(status: u16) -> ErrorCode {
        let known = ErrorCode::ALL
            .into_iter()
            .find(|code| code.http_status() == status);
        match known {
            Some(code) => code,
            None if status < 500 => ErrorCode::BadData,
            None => ErrorCode::Internal,
        }
    }

    pub fn is_client_fault(self) -> bool {
        self.http_status() < 500
    }

    pub fn is_temporary(self) -> bool {
        matches!(self, ErrorCode::Unavailable | ErrorCode::Timeout)
    }

    pub fn public_message(self) -> &'static str {
        match self {
            ErrorCode::Syntax => "тело запроса не JSON",
            ErrorCode::Truncated => "тело запроса оборвано",
            ErrorCode::BadEnvelope => "конверт запроса не той формы",
            ErrorCode::UnknownMethod => "такого метода нет",
            ErrorCode::WrongService => "метод не из этого сервиса",
            ErrorCode::BadData => "данные запроса не той формы",
            ErrorCode::Validation => "проверь поля",
            ErrorCode::Unauthorized => "нужно войти",
            ErrorCode::Forbidden => "нет прав",
            ErrorCode::NotFound => "не найдено",
            ErrorCode::Conflict => "уже занято",
            ErrorCode::TooLarge => "слишком большой запрос",
            ErrorCode::Unsupported => "такой тип не поддерживается",
            ErrorCode::Rejected => "так нельзя",
            ErrorCode::Unavailable => "сервис временно недоступен, повтори позже",
            ErrorCode::Timeout => "сервис не успел ответить, повтори позже",
            ErrorCode::Internal => "внутренняя ошибка",
        }
    }
}

impl PartialEq<&str> for ErrorCode {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl std::fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Detail {
    pub field: String,
    pub code: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorBody {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub details: Vec<Detail>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
}

impl ErrorBody {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> ErrorBody {
        ErrorBody {
            code: code.as_str().to_string(),
            message: message.into(),
            details: Vec::new(),
            request_id: None,
        }
    }

    pub fn public(code: ErrorCode) -> ErrorBody {
        ErrorBody::new(code, code.public_message())
    }

    pub fn invalid(invalid: &Invalid) -> ErrorBody {
        let mut body = ErrorBody::public(ErrorCode::Validation);
        body.details = invalid
            .errors
            .iter()
            .map(|error| Detail {
                field: error.path.clone(),
                code: error.code.to_string(),
                message: error.message.clone(),
            })
            .collect();
        body
    }

    pub fn with_request_id(mut self, request_id: &str) -> ErrorBody {
        self.request_id = Some(request_id.to_string());
        self
    }

    pub fn known(&self) -> Option<ErrorCode> {
        ErrorCode::from_wire(&self.code)
    }

    pub fn http_status(&self) -> u16 {
        self.known().map_or(500, ErrorCode::http_status)
    }

    pub fn fields(&self) -> Vec<(&str, &str)> {
        self.details
            .iter()
            .map(|detail| (detail.field.as_str(), detail.code.as_str()))
            .collect()
    }
}
