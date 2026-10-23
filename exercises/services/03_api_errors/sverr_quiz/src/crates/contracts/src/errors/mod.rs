pub mod code;
pub mod grpc;
pub mod http;

pub use code::Code;
pub use grpc::{GrpcCode, GrpcStatus, from_status, grpc_status};
pub use http::{HttpReply, http_reply};

#[derive(Debug, Clone, PartialEq)]
pub struct Detail {
    pub field: String,
    pub code: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ApiError {
    pub code: Code,
    pub message: Option<String>,
    pub details: Vec<Detail>,
    pub retry_after_secs: Option<u64>,
}

impl ApiError {
    pub fn new(code: Code) -> ApiError {
        ApiError { code, message: None, details: Vec::new(), retry_after_secs: None }
    }

    pub fn with_message(mut self, message: &str) -> ApiError {
        self.message = Some(message.to_string());
        self
    }

    pub fn with_detail(mut self, field: &str, code: &str) -> ApiError {
        self.details.push(Detail { field: field.to_string(), code: code.to_string() });
        self
    }

    pub fn with_retry_after(mut self, secs: u64) -> ApiError {
        self.retry_after_secs = Some(secs);
        self
    }
}
