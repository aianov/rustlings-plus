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
    todo!()
}

pub fn from_status(status: &GrpcStatus) -> ApiError {
    todo!()
}
