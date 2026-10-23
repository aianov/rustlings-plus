use proto::social::v1::{Account, Changed, Post, Profile, RelationsReply};
use proto::{
    AUTH, AVATAR_MAX, ERROR_CODE, MESSAGE_MAX, POSTS, REQUEST_ID, USERS, domain, domain_code,
    explain, internal, new_request_id, request_id, traced,
};
use tonic::Code;

#[test]
fn generated_messages_are_plain_structs() {
    let profile = Profile {
        id: 7,
        username: "alice".to_string(),
        ..Profile::default()
    };
    let account = Account {
        user_id: profile.id,
        username: profile.username.clone(),
        ..Account::default()
    };
    let post = Post {
        author_id: account.user_id,
        text: "привет".to_string(),
        ..Post::default()
    };
    assert_eq!((post.author_id, post.likes), (7, 0));
    assert!(!Changed::default().changed);
    assert!(RelationsReply::default().audience.is_empty());
}

#[test]
fn domain_codes_map_to_grpc_codes() {
    let table = [
        ("not_found", Code::NotFound),
        ("conflict", Code::AlreadyExists),
        ("forbidden", Code::PermissionDenied),
        ("unauthorized", Code::Unauthenticated),
        ("unavailable", Code::Unavailable),
        ("rejected", Code::FailedPrecondition),
        ("bad_data", Code::InvalidArgument),
        ("too_large", Code::InvalidArgument),
    ];
    for (code, grpc) in table {
        let status = domain(code, "понятный текст");
        assert_eq!(status.code(), grpc, "{code} становится {grpc:?}");
        assert_eq!(
            domain_code(&status),
            Some(code),
            "точный код домена едет в метаданных {ERROR_CODE}"
        );
        assert_eq!(status.message(), "понятный текст");
    }
}

#[test]
fn internal_errors_hide_their_text() {
    let status = domain("internal", "SELECT * FROM secrets упал");
    assert_eq!(status.code(), Code::Internal);
    assert!(
        !status.message().contains("secrets"),
        "внутренний текст не уходит по сети: {status:?}"
    );
    assert_eq!(domain_code(&status), None);
    assert_eq!(internal().code(), Code::Internal);
    let leaked = tonic::Status::internal("пароль от базы hunter2");
    let (code, message) = explain(&leaked, "сервис профилей");
    assert_eq!(code, "internal");
    assert!(
        !message.contains("hunter2"),
        "чужой Status::internal без x-error-code тоже не пересказывается: {message}"
    );
}

#[test]
fn explain_turns_status_into_a_client_error() {
    let (code, message) = explain(&domain("not_found", "поста 5 нет"), "сервис постов");
    assert_eq!(
        (code.as_str(), message.as_str()),
        ("not_found", "поста 5 нет")
    );
    for (grpc, expected) in [
        (Code::Unavailable, "unavailable"),
        (Code::DeadlineExceeded, "timeout"),
        (Code::Cancelled, "unavailable"),
    ] {
        let (code, message) = explain(
            &tonic::Status::new(grpc, "tcp connect error"),
            "сервис постов",
        );
        assert_eq!(code, expected, "{grpc:?}");
        assert!(
            message.contains("сервис постов") && !message.contains("tcp"),
            "клиенту говорят, кто недоступен, без кишок транспорта: {message}"
        );
    }
}

#[test]
fn request_id_rides_in_metadata() {
    let first = new_request_id();
    let second = new_request_id();
    assert!(
        first.len() == 16 && first.chars().all(|c| c.is_ascii_hexdigit()),
        "x-request-id это 8 случайных байт в hex: {first}"
    );
    assert_ne!(first, second, "каждый запрос со своим id");
    let request = traced(Changed { changed: true }, &first);
    assert_eq!(
        request
            .metadata()
            .get(REQUEST_ID)
            .and_then(|v| v.to_str().ok()),
        Some(first.as_str())
    );
    assert_eq!(request_id(request.metadata()), first);
    let bare = tonic::Request::new(Changed::default());
    assert_eq!(request_id(bare.metadata()), "-", "без id в логе прочерк");
}

#[test]
fn endpoints_have_names_and_defaults() {
    let all = [AUTH, USERS, POSTS];
    let envs: Vec<&str> = all.iter().map(|e| e.env).collect();
    assert_eq!(
        envs,
        ["AUTH_GRPC_ADDR", "USERS_GRPC_ADDR", "POSTS_GRPC_ADDR"]
    );
    let defaults: Vec<&str> = all.iter().map(|e| e.default).collect();
    assert_eq!(
        defaults,
        ["127.0.0.1:50051", "127.0.0.1:50052", "127.0.0.1:50053"]
    );
    assert_eq!(
        (AVATAR_MAX, MESSAGE_MAX - AVATAR_MAX),
        (8 * 1024 * 1024, 64 * 1024),
        "аватар до 8 МиБ, вдвое больше стандартных 4 МиБ gRPC, и запас 64 КиБ на остальные поля"
    );
}
