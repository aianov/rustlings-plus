use std::fmt::Write;

use contracts::{Detail, ErrorBody, ErrorCode};
use tonic::metadata::{MetadataMap, MetadataValue};
use tonic::{Code, Request, Status};
use validation::Invalid;

pub const ERROR_CODE: &str = "x-error-code";

pub const ERROR_DETAILS: &str = "x-error-details-bin";

pub const REQUEST_ID: &str = "x-request-id";

const INTERNAL: &str = "внутренняя ошибка сервиса";

pub fn internal() -> Status {
    Status::internal(INTERNAL)
}

pub fn grpc_code(code: ErrorCode) -> Code {
    match code {
        ErrorCode::NotFound => Code::NotFound,
        ErrorCode::Conflict => Code::AlreadyExists,
        ErrorCode::Forbidden => Code::PermissionDenied,
        ErrorCode::Unauthorized | ErrorCode::Expired => Code::Unauthenticated,
        ErrorCode::Unavailable => Code::Unavailable,
        ErrorCode::Timeout => Code::DeadlineExceeded,
        ErrorCode::Rejected => Code::FailedPrecondition,
        ErrorCode::Internal => Code::Internal,
        ErrorCode::Syntax
        | ErrorCode::Truncated
        | ErrorCode::BadEnvelope
        | ErrorCode::UnknownMethod
        | ErrorCode::WrongService
        | ErrorCode::BadData
        | ErrorCode::Validation
        | ErrorCode::TooLarge
        | ErrorCode::Unsupported => Code::InvalidArgument,
    }
}

fn encode_details(details: &[Detail]) -> Vec<u8> {
    let clean = |text: &str| text.replace(['\t', '\n'], " ");
    let mut out = String::new();
    for detail in details {
        let _ = writeln!(
            out,
            "{}\t{}\t{}",
            clean(&detail.field),
            clean(&detail.code),
            clean(&detail.message)
        );
    }
    out.into_bytes()
}

fn decode_details(status: &Status) -> Vec<Detail> {
    let Some(raw) = status.metadata().get_bin(ERROR_DETAILS) else {
        return Vec::new();
    };
    let Ok(bytes) = raw.to_bytes() else {
        return Vec::new();
    };
    String::from_utf8_lossy(&bytes)
        .lines()
        .filter_map(|line| {
            let mut parts = line.splitn(3, '\t');
            let field = parts.next()?.to_string();
            let code = parts.next()?.to_string();
            let message = parts.next().unwrap_or_default().to_string();
            Some(Detail {
                field,
                code,
                message,
            })
        })
        .collect()
}

pub fn to_status(body: &ErrorBody) -> Status {
    let known = body.known();
    if known == Some(ErrorCode::Internal) {
        return internal();
    }
    let grpc = known.map_or(Code::InvalidArgument, grpc_code);
    let mut status = Status::new(grpc, body.message.clone());
    if let Ok(value) = body.code.parse() {
        status.metadata_mut().insert(ERROR_CODE, value);
    }
    if !body.details.is_empty() {
        let value = MetadataValue::from_bytes(&encode_details(&body.details));
        status.metadata_mut().insert_bin(ERROR_DETAILS, value);
    }
    status
}

pub fn domain(code: &str, message: &str) -> Status {
    let mut body = ErrorBody::public(ErrorCode::Internal);
    body.code = code.to_string();
    body.message = message.to_string();
    to_status(&body)
}

pub fn invalid(invalid: &Invalid) -> Status {
    to_status(&ErrorBody::invalid(invalid))
}

pub fn domain_code(status: &Status) -> Option<&str> {
    status
        .metadata()
        .get(ERROR_CODE)?
        .to_str()
        .ok()
        .filter(|code| *code != "internal")
}

fn announced(status: &Status) -> Option<ErrorCode> {
    domain_code(status)
        .and_then(ErrorCode::from_wire)
        .filter(|code| grpc_code(*code) == status.code())
}

pub fn from_status(status: &Status, service: &str) -> ErrorBody {
    match announced(status) {
        Some(code) if code.is_client_fault() => ErrorBody {
            details: decode_details(status),
            ..ErrorBody::new(code, status.message())
        },
        Some(ErrorCode::Unavailable) => unavailable(service),
        Some(ErrorCode::Timeout) => timeout(service),
        Some(..) | None => match status.code() {
            Code::Unavailable | Code::Cancelled => unavailable(service),
            Code::DeadlineExceeded => timeout(service),
            _ => ErrorBody::new(ErrorCode::Internal, INTERNAL),
        },
    }
}

fn unavailable(service: &str) -> ErrorBody {
    ErrorBody::new(
        ErrorCode::Unavailable,
        format!("{service} недоступен, повтори позже"),
    )
}

fn timeout(service: &str) -> ErrorBody {
    ErrorBody::new(
        ErrorCode::Timeout,
        format!("{service} не успел ответить, повтори позже"),
    )
}

pub fn explain(status: &Status, service: &str) -> (String, String) {
    let body = from_status(status, service);
    (body.code, body.message)
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
