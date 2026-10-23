
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrpcCode {
    InvalidArgument,
    NotFound,
    PermissionDenied,
    Unauthenticated,
    ResourceExhausted,
    Internal,
    Unavailable,
    DeadlineExceeded,
}

#[derive(Debug, Clone)]
pub struct Status {
    pub code: GrpcCode,
    pub message: String,
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, PartialEq)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: serde_json::Value,
}

pub const PUBLIC_CODES: [(&str, GrpcCode); 4] = [
    ("post.not_found", GrpcCode::NotFound),
    ("post.author_not_found", GrpcCode::InvalidArgument),
    ("validation.failed", GrpcCode::InvalidArgument),
    ("rate_limited", GrpcCode::ResourceExhausted),
];

pub fn public_message(code: &str) -> &'static str {
    match code {
        "post.not_found" => "пост не найден",
        "post.author_not_found" => "автор не найден",
        "validation.failed" => "проверь поля",
        "rate_limited" => "слишком много запросов",
        "service.unavailable" => "сервис временно недоступен",
        "service.timeout" => "сервис не успел ответить",
        _ => "внутренняя ошибка",
    }
}

fn http_and_default(code: GrpcCode) -> (u16, &'static str) {
    match code {
        GrpcCode::InvalidArgument => (422, "validation.failed"),
        GrpcCode::NotFound => (404, "post.not_found"),
        GrpcCode::PermissionDenied | GrpcCode::Unauthenticated => (500, "internal"),
        GrpcCode::ResourceExhausted => (429, "rate_limited"),
        GrpcCode::Unavailable => (503, "service.unavailable"),
        GrpcCode::DeadlineExceeded => (504, "service.timeout"),
        GrpcCode::Internal => (500, "internal"),
    }
}

pub fn forward_error(status: &Status, request_id: &str, log: &mut Vec<String>) -> Response {
    let (http, default_code) = http_and_default(status.code);
    let code = status
        .metadata
        .get("x-error-code")
        .map(String::as_str)
        .filter(|c| PUBLIC_CODES.contains(&(*c, status.code)))
        .unwrap_or(default_code);
    log.push(format!(
        "request_id={request_id} upstream_request_id={} upstream_code={:?} upstream_message={}",
        status.metadata.get("x-request-id").map(String::as_str).unwrap_or("-"),
        status.code,
        status.message
    ));
    let mut headers = Vec::new();
    if matches!(http, 429 | 503) {
        if let Some(seconds) = status.metadata.get("retry-after") {
            headers.push(("retry-after".to_string(), seconds.clone()));
        }
    }
    Response {
        status: http,
        headers,
        body: serde_json::json!({"error": {"code": code, "message": public_message(code), "request_id": request_id}}),
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(code: GrpcCode, message: &str, meta: &[(&str, &str)]) -> Status {
        Status {
            code,
            message: message.to_string(),
            metadata: meta.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
        }
    }

    fn forward(s: &Status) -> (Response, Vec<String>) {
        let mut log = Vec::new();
        let response = forward_error(s, "gw-1", &mut log);
        (response, log)
    }

    #[test]
    fn public_codes_go_through() {
        let s = status(GrpcCode::NotFound, "no row", &[("x-error-code", "post.not_found")]);
        let (response, _) = forward(&s);
        assert_eq!(response.status, 404, "статус по смыслу gRPC-кода");
        assert_eq!(response.body["error"]["code"], "post.not_found", "публичный код соседа переходит к клиенту");
        assert_eq!(response.body["error"]["message"], "пост не найден", "а сообщение берётся из нашего каталога");
    }

    #[test]
    fn private_codes_are_replaced() {
        let s = status(GrpcCode::Unavailable, "x", &[("x-error-code", "internal.pg_pool_exhausted")]);
        let (response, _) = forward(&s);
        assert_eq!(response.status, 503, "статус остаётся");
        assert_eq!(response.body["error"]["code"], "service.unavailable", "незнакомый клиенту код заменяется кодом по умолчанию для этого типа ошибки");
    }

    #[test]
    fn service_to_service_auth_failures_are_our_bug_not_the_clients() {
        for code in [GrpcCode::PermissionDenied, GrpcCode::Unauthenticated] {
            let (response, _) = forward(&status(code, "token for gateway expired", &[]));
            assert_eq!(response.status, 500, "{code:?} от соседа при служебном вызове значит, что сломана наша настройка, а не права клиента: 403 и 401 здесь ложь");
            assert_eq!(response.body["error"]["code"], "internal", "{code:?}: код internal");
        }
    }

    #[test]
    fn code_must_agree_with_the_grpc_status() {
        let lying = status(GrpcCode::Internal, "x", &[("x-error-code", "post.not_found")]);
        let (response, _) = forward(&lying);
        assert_eq!(response.status, 500, "статус остаётся 500");
        assert_eq!(response.body["error"]["code"], "internal", "код 404-й ошибки при статусе Internal противоречит сам себе: верим gRPC-коду");
    }

    #[test]
    fn neighbours_words_never_reach_the_client() {
        let text = "connection refused to 10.0.3.7:5432 (posts_pg)";
        for code in [GrpcCode::Unavailable, GrpcCode::Internal, GrpcCode::NotFound, GrpcCode::PermissionDenied, GrpcCode::InvalidArgument] {
            let (response, _) = forward(&status(code, text, &[("x-request-id", "post-55")]));
            let sent = format!("{:?} {}", response.headers, response.body);
            assert!(!sent.contains("10.0.3.7") && !sent.contains("posts_pg"), "{code:?}: подробности соседа ушли клиенту: {sent}");
            assert!(!sent.contains("post-55"), "{code:?}: внутренний номер запроса соседа клиенту не нужен: {sent}");
        }
    }

    #[test]
    fn request_id_is_ours_and_neighbours_goes_to_the_log() {
        let s = status(GrpcCode::Unavailable, "boom", &[("x-request-id", "post-55")]);
        let (response, log) = forward(&s);
        assert_eq!(response.body["error"]["request_id"], "gw-1", "клиент жалуется по номеру шлюза");
        assert_eq!(log.len(), 1, "одна запись: {log:?}");
        for needle in ["gw-1", "post-55", "boom"] {
            assert!(log[0].contains(needle), "в логе нет «{needle}», а связать запись шлюза с записью соседа и увидеть подробности нужно: {}", log[0]);
        }
    }

    #[test]
    fn retry_after_survives_for_temporary_errors_only() {
        let s = status(GrpcCode::ResourceExhausted, "x", &[("retry-after", "12")]);
        let (response, _) = forward(&s);
        assert_eq!(response.status, 429, "лимит");
        assert!(response.headers.contains(&("retry-after".to_string(), "12".to_string())), "заголовок retry-after переехал: {:?}", response.headers);
        let s = status(GrpcCode::NotFound, "x", &[("retry-after", "12")]);
        let (response, _) = forward(&s);
        assert!(response.headers.is_empty(), "для 404 retry-after не имеет смысла: {:?}", response.headers);
    }

    #[test]
    fn missing_metadata_is_fine() {
        let (response, _) = forward(&status(GrpcCode::DeadlineExceeded, "slow", &[]));
        assert_eq!(response.status, 504, "таймаут");
        assert_eq!(response.body["error"]["code"], "service.timeout", "код по умолчанию");
        assert_eq!(response.body["error"]["request_id"], "gw-1", "номер свой и без метаданных");
    }
}
