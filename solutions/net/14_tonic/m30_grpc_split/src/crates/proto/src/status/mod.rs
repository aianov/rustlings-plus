use std::fmt::Write;

use tonic::metadata::MetadataMap;
use tonic::{Code, Request, Status};

pub const ERROR_CODE: &str = "x-error-code";

pub const REQUEST_ID: &str = "x-request-id";

const INTERNAL: &str = "внутренняя ошибка сервиса";

pub fn internal() -> Status {
    Status::internal(INTERNAL)
}

pub fn domain(code: &str, message: &str) -> Status {
    let grpc = match code {
        "not_found" => Code::NotFound,
        "conflict" => Code::AlreadyExists,
        "forbidden" => Code::PermissionDenied,
        "unauthorized" => Code::Unauthenticated,
        "unavailable" => Code::Unavailable,
        "rejected" => Code::FailedPrecondition,
        "internal" => return internal(),
        _ => Code::InvalidArgument,
    };
    let mut status = Status::new(grpc, message);
    if let Ok(value) = code.parse() {
        status.metadata_mut().insert(ERROR_CODE, value);
    }
    status
}

pub fn domain_code(status: &Status) -> Option<&str> {
    status
        .metadata()
        .get(ERROR_CODE)?
        .to_str()
        .ok()
        .filter(|code| *code != "internal")
}

pub fn explain(status: &Status, service: &str) -> (String, String) {
    if let Some(code) = domain_code(status) {
        return (code.to_string(), status.message().to_string());
    }
    match status.code() {
        Code::Unavailable | Code::DeadlineExceeded | Code::Cancelled => (
            String::from("unavailable"),
            format!("{service} недоступен, повтори позже"),
        ),
        _ => (String::from("internal"), String::from(INTERNAL)),
    }
}

pub fn request_id(metadata: &MetadataMap) -> String {
    metadata
        .get(REQUEST_ID)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("-")
        .to_string()
}

pub fn new_request_id() -> String {
    let mut raw = [0u8; 8];
    if getrandom::fill(&mut raw).is_err() {
        return String::from("-");
    }
    let mut id = String::with_capacity(16);
    for byte in raw {
        let _ = write!(id, "{byte:02x}");
    }
    id
}

pub fn traced<T>(message: T, request_id: &str) -> Request<T> {
    let mut request = Request::new(message);
    if let Ok(value) = request_id.parse() {
        request.metadata_mut().insert(REQUEST_ID, value);
    }
    request
}
