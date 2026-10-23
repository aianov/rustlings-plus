use super::{ApiError, Code};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrpcCode {
    InvalidArgument,
    NotFound,
    AlreadyExists,
    PermissionDenied,
    ResourceExhausted,
    Internal,
    Unavailable,
    DeadlineExceeded,
    Unauthenticated,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GrpcStatus {
    pub code: GrpcCode,
    pub message: String,
    pub metadata: BTreeMap<String, String>,
}

impl GrpcStatus {
    pub fn new(code: GrpcCode, message: &str) -> GrpcStatus {
        GrpcStatus { code, message: message.to_string(), metadata: BTreeMap::new() }
    }

    pub fn with_meta(mut self, key: &str, value: &str) -> GrpcStatus {
        self.metadata.insert(key.to_string(), value.to_string());
        self
    }
}

pub const CODE_KEY: &str = "x-error-code";
pub const REQUEST_ID_KEY: &str = "x-request-id";
pub const DETAILS_KEY: &str = "x-error-details";
pub const RETRY_AFTER_KEY: &str = "retry-after";

pub fn grpc_status(err: &ApiError, request_id: &str) -> GrpcStatus {
    let message = match (&err.message, err.code.is_client_fault()) {
        (Some(text), true) => text.as_str(),
        _ => err.code.public_message(),
    };
    let mut status = GrpcStatus::new(err.code.grpc(), message)
        .with_meta(CODE_KEY, err.code.as_str())
        .with_meta(REQUEST_ID_KEY, request_id);
    if err.code.is_client_fault() && !err.details.is_empty() {
        let details: Vec<String> = err.details.iter().map(|d| format!("{}:{}", d.field, d.code)).collect();
        status = status.with_meta(DETAILS_KEY, &details.join(","));
    }
    if let Some(secs) = err.retry_after_secs {
        status = status.with_meta(RETRY_AFTER_KEY, &secs.to_string());
    }
    status
}

pub fn from_status(status: &GrpcStatus) -> ApiError {
    let announced = status.metadata.get(CODE_KEY).and_then(|text| Code::from_wire(text)).filter(|code| code.grpc() == status.code);
    let code = announced.unwrap_or(match status.code {
        GrpcCode::Unavailable => Code::ServiceUnavailable,
        GrpcCode::DeadlineExceeded => Code::Timeout,
        GrpcCode::ResourceExhausted => Code::RateLimited,
        _ => Code::Internal,
    });
    let mut err = ApiError::new(code);
    if code.is_client_fault() {
        if let Some(text) = status.metadata.get(DETAILS_KEY) {
            for pair in text.split(',') {
                if let Some((field, detail)) = pair.split_once(':') {
                    err = err.with_detail(field, detail);
                }
            }
        }
    }
    if let Some(secs) = status.metadata.get(RETRY_AFTER_KEY).and_then(|text| text.parse().ok()) {
        err.retry_after_secs = Some(secs);
    }
    err
}
