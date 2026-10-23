use contracts::{ErrorBody, ErrorCode};
use proto::{ERROR_CODE, ERROR_DETAILS, from_status, grpc_code, invalid, to_status};
use tonic::metadata::MetadataValue;
use tonic::{Code, Status};
use validation::{Validator, Violation};

const USERS: &str = "сервис профилей";

#[test]
fn every_catalogue_code_has_a_grpc_code() {
    let table: Vec<(&str, Code)> = [
        ErrorCode::Validation,
        ErrorCode::BadData,
        ErrorCode::NotFound,
        ErrorCode::Conflict,
        ErrorCode::Forbidden,
        ErrorCode::Unauthorized,
        ErrorCode::Rejected,
        ErrorCode::Unavailable,
        ErrorCode::Timeout,
        ErrorCode::Internal,
    ]
    .into_iter()
    .map(|code| (code.as_str(), grpc_code(code)))
    .collect();
    assert_eq!(
        table,
        [
            ("validation", Code::InvalidArgument),
            ("bad_data", Code::InvalidArgument),
            ("not_found", Code::NotFound),
            ("conflict", Code::AlreadyExists),
            ("forbidden", Code::PermissionDenied),
            ("unauthorized", Code::Unauthenticated),
            ("rejected", Code::FailedPrecondition),
            ("unavailable", Code::Unavailable),
            ("timeout", Code::DeadlineExceeded),
            ("internal", Code::Internal),
        ]
    );
}

#[test]
fn field_errors_travel_through_grpc_and_back() {
    let mut check = Validator::new();
    check.report(
        "username",
        Violation::new("too_short", "не короче 3 символов"),
    );
    check.report(
        "links[1].url",
        Violation::new("invalid_url", "нужна ссылка: http или https"),
    );
    let errors = check.finish().expect_err("две ошибки");
    let status = invalid(&errors);
    assert_eq!(status.code(), Code::InvalidArgument);
    assert_eq!(
        status
            .metadata()
            .get(ERROR_CODE)
            .and_then(|v| v.to_str().ok()),
        Some("validation")
    );
    assert!(
        status.metadata().get_bin(ERROR_DETAILS).is_some(),
        "детали едут в бинарных метаданных: в обычных разрешён только ASCII, а сообщения русские"
    );
    let body = from_status(&status, USERS);
    assert_eq!(body.code, "validation");
    assert_eq!(
        body.fields(),
        [("username", "too_short"), ("links[1].url", "invalid_url")]
    );
    assert_eq!(body.details[1].message, "нужна ссылка: http или https");
}

#[test]
fn internal_text_never_leaves_the_service() {
    let leaky = to_status(&ErrorBody::new(
        ErrorCode::Internal,
        "SELECT * FROM secrets: hunter2",
    ));
    assert_eq!(leaky.code(), Code::Internal);
    assert!(!leaky.message().contains("hunter2"), "{leaky:?}");
    let foreign = from_status(&Status::internal("пароль от базы hunter2"), USERS);
    assert_eq!(foreign.code, "internal");
    assert!(!foreign.message.contains("hunter2"), "{foreign:?}");
    let mut details = Status::internal("сломалось");
    details
        .metadata_mut()
        .insert(ERROR_CODE, MetadataValue::from_static("validation"));
    let contradicting = from_status(&details, USERS);
    assert_eq!(
        contradicting.code, "internal",
        "код validation при статусе Internal противоречит сам себе: верим gRPC-коду"
    );
    let mut unknown = Status::not_found("нет");
    unknown
        .metadata_mut()
        .insert(ERROR_CODE, MetadataValue::from_static("weird"));
    assert_eq!(
        from_status(&unknown, USERS).code,
        "internal",
        "незнакомый код соседа не пересказываем"
    );
}

#[test]
fn transport_failures_are_temporary_and_name_the_service() {
    let down = from_status(&Status::unavailable("tcp connect error"), USERS);
    assert_eq!(
        (down.code.as_str(), down.http_status()),
        ("unavailable", 503)
    );
    assert!(
        down.message.contains(USERS) && !down.message.contains("tcp"),
        "{down:?}"
    );
    let slow = from_status(&Status::deadline_exceeded("timeout expired"), USERS);
    assert_eq!((slow.code.as_str(), slow.http_status()), ("timeout", 504));
    let domain = from_status(
        &to_status(&ErrorBody::new(
            ErrorCode::NotFound,
            "пользователя @bob нет",
        )),
        USERS,
    );
    assert_eq!(
        (domain.code.as_str(), domain.message.as_str()),
        ("not_found", "пользователя @bob нет"),
        "доменная ошибка соседа доезжает со своим текстом: его собрал наш сервис из безопасных кусочков"
    );
}
