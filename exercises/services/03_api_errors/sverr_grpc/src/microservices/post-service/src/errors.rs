use crate::domain::PostError;
use tonic::{Request, Status};

pub const CODE_KEY: &str = "x-error-code";
pub const REQUEST_ID_KEY: &str = "x-request-id";
pub const DETAILS_KEY: &str = "x-error-details";
pub const RETRY_AFTER_KEY: &str = "retry-after";

pub fn request_id<T>(request: &Request<T>) -> String {
    todo!()
}

pub fn status_from(err: &PostError, request_id: &str) -> Status {
    todo!()
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

pub fn client_error(status: &Status) -> ClientError {
    todo!()
}
