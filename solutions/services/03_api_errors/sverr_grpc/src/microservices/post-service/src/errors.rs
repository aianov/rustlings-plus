use crate::domain::PostError;
use std::sync::atomic::{AtomicU64, Ordering};
use tonic::metadata::MetadataValue;
use tonic::{Code, Request, Status};

pub const CODE_KEY: &str = "x-error-code";
pub const REQUEST_ID_KEY: &str = "x-request-id";
pub const DETAILS_KEY: &str = "x-error-details";
pub const RETRY_AFTER_KEY: &str = "retry-after";

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

pub fn request_id<T>(request: &Request<T>) -> String {
    request
        .metadata()
        .get(REQUEST_ID_KEY)
        .and_then(|value| value.to_str().ok())
        .filter(|text| !text.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("gen-{}", NEXT_ID.fetch_add(1, Ordering::Relaxed)))
}

fn put(status: &mut Status, key: &'static str, value: &str) {
    if let Ok(value) = MetadataValue::try_from(value) {
        status.metadata_mut().insert(key, value);
    }
}

pub fn status_from(err: &PostError, request_id: &str) -> Status {
    let (code, message, app_code) = match err {
        PostError::NotFound(id) => (Code::NotFound, format!("пост {id} не найден"), "post.not_found"),
        PostError::AuthorNotFound(id) => (Code::InvalidArgument, format!("автор {id} не найден"), "post.author_not_found"),
        PostError::TextTooLong { .. } => (Code::InvalidArgument, "текст слишком длинный".to_string(), "validation.failed"),
        PostError::Unavailable => (Code::Unavailable, "сервис временно недоступен".to_string(), "service.unavailable"),
        PostError::Internal(_) => (Code::Internal, "внутренняя ошибка".to_string(), "internal"),
    };
    let mut status = Status::new(code, message);
    put(&mut status, CODE_KEY, app_code);
    put(&mut status, REQUEST_ID_KEY, request_id);
    match err {
        PostError::TextTooLong { .. } => put(&mut status, DETAILS_KEY, "text:too_long"),
        PostError::Unavailable => put(&mut status, RETRY_AFTER_KEY, "1"),
        _ => {}
    }
    status
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClientError {
    pub code: String,
    pub message: String,
    pub request_id: Option<String>,
    pub retryable: bool,
    pub retry_after_secs: Option<u64>,
    pub details: Vec<(String, String)>,
}

fn meta(status: &Status, key: &str) -> Option<String> {
    status.metadata().get(key).and_then(|value| value.to_str().ok()).map(str::to_string)
}

pub fn client_error(status: &Status) -> ClientError {
    let fallback = match status.code() {
        Code::Unavailable => "service.unavailable",
        Code::DeadlineExceeded => "service.timeout",
        Code::NotFound => "not_found",
        Code::InvalidArgument => "validation.failed",
        _ => "internal",
    };
    let details = meta(status, DETAILS_KEY)
        .map(|text| {
            text.split(',')
                .filter_map(|pair| pair.split_once(':'))
                .map(|(field, code)| (field.to_string(), code.to_string()))
                .collect()
        })
        .unwrap_or_default();
    ClientError {
        code: meta(status, CODE_KEY).unwrap_or_else(|| fallback.to_string()),
        message: status.message().to_string(),
        request_id: meta(status, REQUEST_ID_KEY),
        retryable: matches!(status.code(), Code::Unavailable | Code::DeadlineExceeded | Code::ResourceExhausted),
        retry_after_secs: meta(status, RETRY_AFTER_KEY).and_then(|text| text.parse().ok()),
        details,
    }
}
