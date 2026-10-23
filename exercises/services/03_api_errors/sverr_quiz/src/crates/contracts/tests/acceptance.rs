use contracts::errors::{ApiError, Code, GrpcCode, GrpcStatus, from_status, grpc_status, http_reply};

#[test]
fn wire_names_are_frozen_and_unique() {
    let expected = [
        (Code::Malformed, "request.malformed"),
        (Code::ValidationFailed, "validation.failed"),
        (Code::Unauthenticated, "auth.unauthenticated"),
        (Code::Forbidden, "auth.forbidden"),
        (Code::UserNotFound, "user.not_found"),
        (Code::UsernameTaken, "user.username_taken"),
        (Code::PostNotFound, "post.not_found"),
        (Code::AuthorNotFound, "post.author_not_found"),
        (Code::RateLimited, "rate_limited"),
        (Code::ServiceUnavailable, "service.unavailable"),
        (Code::Timeout, "service.timeout"),
        (Code::Internal, "internal"),
    ];
    for (code, wire) in expected {
        assert_eq!(code.as_str(), wire, "строка кода это контракт: {code:?}");
        assert_eq!(Code::from_wire(wire), Some(code), "обратный разбор {wire}");
    }
    assert_eq!(Code::ALL.len(), expected.len(), "ALL содержит все коды");
    assert_eq!(Code::from_wire("post.archived"), None, "незнакомая строка это None, а не паника и не Internal");
}

#[test]
fn http_and_grpc_tables() {
    let expected = [
        (Code::Malformed, 400, GrpcCode::InvalidArgument),
        (Code::ValidationFailed, 422, GrpcCode::InvalidArgument),
        (Code::Unauthenticated, 401, GrpcCode::Unauthenticated),
        (Code::Forbidden, 403, GrpcCode::PermissionDenied),
        (Code::UserNotFound, 404, GrpcCode::NotFound),
        (Code::UsernameTaken, 409, GrpcCode::AlreadyExists),
        (Code::PostNotFound, 404, GrpcCode::NotFound),
        (Code::AuthorNotFound, 422, GrpcCode::InvalidArgument),
        (Code::RateLimited, 429, GrpcCode::ResourceExhausted),
        (Code::ServiceUnavailable, 503, GrpcCode::Unavailable),
        (Code::Timeout, 504, GrpcCode::DeadlineExceeded),
        (Code::Internal, 500, GrpcCode::Internal),
    ];
    for (code, http, grpc) in expected {
        assert_eq!(code.http_status(), http, "HTTP-статус для {code:?}");
        assert_eq!(code.grpc(), grpc, "gRPC-код для {code:?}");
        assert_eq!(code.is_client_fault(), http < 500, "чья вина: {code:?}");
        assert!(!code.public_message().is_empty(), "у {code:?} есть публичное сообщение");
    }
}

#[test]
fn http_body_is_the_envelope() {
    let err = ApiError::new(Code::ValidationFailed).with_detail("text", "required").with_detail("author_id", "required");
    let reply = http_reply(&err, "req-1");
    assert_eq!(reply.status, 422, "статус");
    assert_eq!(reply.header("content-type"), Some("application/json"), "тип содержимого");
    assert_eq!(reply.header("x-request-id"), Some("req-1"), "номер запроса и в заголовке");
    let body = reply.json();
    assert_eq!(body["error"]["code"], "validation.failed", "стабильный код");
    assert_eq!(body["error"]["request_id"], "req-1", "номер запроса в теле");
    assert_eq!(
        body["error"]["details"],
        serde_json::json!([{"field": "text", "code": "required"}, {"field": "author_id", "code": "required"}]),
        "details в порядке добавления"
    );
    let plain = http_reply(&ApiError::new(Code::PostNotFound), "r").json();
    assert!(plain["error"].get("details").is_none(), "пустых details в JSON нет");
}

#[test]
fn only_client_faults_may_carry_custom_text() {
    let reply = http_reply(&ApiError::new(Code::UserNotFound).with_message("пользователь 7 не найден"), "r");
    assert_eq!(reply.json()["error"]["message"], "пользователь 7 не найден", "для 4xx своё сообщение допустимо");
    let reply = http_reply(&ApiError::new(Code::Internal).with_message("password=hunter2 host=db-7"), "r");
    assert_eq!(reply.json()["error"]["message"], Code::Internal.public_message(), "для 5xx берётся только публичный текст, что бы ни положили в message");
    assert!(!reply.body.contains("hunter2"), "и ничего больше из message: {}", reply.body);
    let reply = http_reply(&ApiError::new(Code::ServiceUnavailable).with_message("db-7 refused").with_detail("db", "down"), "r");
    assert!(!reply.body.contains("db-7") && !reply.body.contains("down"), "ни текст, ни details у 5xx не показываются: {}", reply.body);
}

#[test]
fn headers_follow_the_status() {
    let reply = http_reply(&ApiError::new(Code::Unauthenticated), "r");
    assert_eq!(reply.header("www-authenticate"), Some("Bearer"), "401 говорит, как войти");
    let reply = http_reply(&ApiError::new(Code::RateLimited).with_retry_after(30), "r");
    assert_eq!(reply.header("retry-after"), Some("30"), "429 с паузой");
    let reply = http_reply(&ApiError::new(Code::ServiceUnavailable).with_retry_after(2), "r");
    assert_eq!(reply.header("retry-after"), Some("2"), "503 с паузой");
    let reply = http_reply(&ApiError::new(Code::PostNotFound).with_retry_after(2), "r");
    assert_eq!(reply.header("retry-after"), None, "404 не повторяют, подсказка не нужна");
    assert_eq!(http_reply(&ApiError::new(Code::Forbidden), "r").header("www-authenticate"), None, "403 не просит войти");
}

#[test]
fn every_code_survives_the_trip_between_services() {
    for code in Code::ALL {
        let status = grpc_status(&ApiError::new(code), "req-9");
        assert_eq!(status.code, code.grpc(), "gRPC-код {code:?}");
        assert_eq!(status.metadata.get("x-error-code").map(String::as_str), Some(code.as_str()), "машинный код в метаданных {code:?}");
        assert_eq!(status.metadata.get("x-request-id").map(String::as_str), Some("req-9"), "номер запроса {code:?}");
        let back = from_status(&status);
        assert_eq!(back.code, code, "после круга код остаётся тем же: {code:?}");
    }
}

#[test]
fn details_and_retry_after_travel_in_metadata() {
    let err = ApiError::new(Code::ValidationFailed).with_detail("text", "too_long").with_detail("author_id", "required");
    let status = grpc_status(&err, "r");
    assert_eq!(status.metadata.get("x-error-details").map(String::as_str), Some("text:too_long,author_id:required"), "формат пар");
    assert_eq!(from_status(&status).details, err.details, "детали разобраны обратно");

    let status = grpc_status(&ApiError::new(Code::ServiceUnavailable).with_retry_after(3), "r");
    assert_eq!(status.metadata.get("retry-after").map(String::as_str), Some("3"), "пауза в метаданных");
    assert_eq!(from_status(&status).retry_after_secs, Some(3), "и обратно");
}

#[test]
fn internal_details_do_not_cross_the_service_border() {
    let status = grpc_status(&ApiError::new(Code::Internal).with_message("dsn=postgres://u:p@db").with_detail("x", "y"), "r");
    assert_eq!(status.message, Code::Internal.public_message(), "наружу идёт только публичный текст");
    assert!(!status.metadata.contains_key("x-error-details"), "details у серверной ошибки не передаются");
}

#[test]
fn foreign_statuses_are_judged_by_their_grpc_code() {
    let from_proxy = GrpcStatus::new(GrpcCode::Unavailable, "upstream connect error");
    assert_eq!(from_status(&from_proxy).code, Code::ServiceUnavailable, "без метаданных Unavailable значит недоступность");
    let slow = GrpcStatus::new(GrpcCode::DeadlineExceeded, "deadline");
    assert_eq!(from_status(&slow).code, Code::Timeout, "DeadlineExceeded значит таймаут");
    let denied = GrpcStatus::new(GrpcCode::PermissionDenied, "token expired");
    assert_eq!(from_status(&denied).code, Code::Internal, "PermissionDenied без нашего кода это неизвестная причина, а не 403 для клиента");
    let unknown_code = GrpcStatus::new(GrpcCode::NotFound, "x").with_meta("x-error-code", "internal.pool");
    assert_eq!(from_status(&unknown_code).code, Code::Internal, "незнакомый код соседа не принимается на веру");
}

#[test]
fn contradicting_metadata_is_not_trusted() {
    let lying = GrpcStatus::new(GrpcCode::Internal, "x").with_meta("x-error-code", "post.not_found");
    assert_eq!(from_status(&lying).code, Code::Internal, "код 404-й ошибки при gRPC Internal не сходится: верим gRPC-коду");
}

#[test]
fn neighbours_message_is_never_adopted() {
    let status = GrpcStatus::new(GrpcCode::NotFound, "connection refused to 10.0.3.7").with_meta("x-error-code", "post.not_found");
    let err = from_status(&status);
    assert_eq!(err.code, Code::PostNotFound, "код принят");
    assert_eq!(err.message, None, "чужой текст в ApiError не попадает");
}
