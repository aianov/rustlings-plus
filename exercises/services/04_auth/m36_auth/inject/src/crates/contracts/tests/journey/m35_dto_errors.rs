use contracts::{
    Detail, Email, ErrorBody, ErrorCode, Field, LinkView, NameError, Outcome, PostId, PostView,
    ProfilePatch, UserId, UserView, decode, parse_body,
};
use serde_json::{Value, json};
use validation::{Invalid, Validator, Violation};

#[test]
fn the_code_catalogue_is_a_public_contract() {
    let wire: Vec<&str> = ErrorCode::ALL.iter().map(|code| code.as_str()).collect();
    assert_eq!(
        wire,
        [
            "syntax",
            "truncated",
            "bad_envelope",
            "unknown_method",
            "wrong_service",
            "bad_data",
            "validation",
            "unauthorized",
            "expired",
            "forbidden",
            "not_found",
            "conflict",
            "too_large",
            "unsupported",
            "rejected",
            "unavailable",
            "timeout",
            "internal",
        ],
        "строки кодов не переименовывают: на них ветвятся чужие программы"
    );
    for code in ErrorCode::ALL {
        assert_eq!(ErrorCode::from_wire(code.as_str()), Some(code));
    }
    assert_eq!(ErrorCode::from_wire("brand_new"), None);
    let statuses: Vec<(&str, u16)> = [
        ErrorCode::Syntax,
        ErrorCode::Validation,
        ErrorCode::Unauthorized,
        ErrorCode::NotFound,
        ErrorCode::Conflict,
        ErrorCode::Rejected,
        ErrorCode::Unavailable,
        ErrorCode::Timeout,
        ErrorCode::Internal,
    ]
    .into_iter()
    .map(|code| (code.as_str(), code.http_status()))
    .collect();
    assert_eq!(
        statuses,
        [
            ("syntax", 400),
            ("validation", 422),
            ("unauthorized", 401),
            ("not_found", 404),
            ("conflict", 409),
            ("rejected", 422),
            ("unavailable", 503),
            ("timeout", 504),
            ("internal", 500),
        ]
    );
    assert!(ErrorCode::Validation.is_client_fault() && !ErrorCode::Timeout.is_client_fault());
    assert!(ErrorCode::Timeout.is_temporary() && !ErrorCode::Internal.is_temporary());
    assert_eq!(ErrorCode::for_status(404), ErrorCode::NotFound);
    assert_eq!(ErrorCode::for_status(405), ErrorCode::BadData);
    assert_eq!(ErrorCode::for_status(502), ErrorCode::Internal);
    assert_eq!(
        ErrorCode::Conflict,
        "conflict",
        "код сравнивается со строкой провода"
    );
}

#[test]
fn the_envelope_grows_without_breaking_old_clients() {
    let plain = ErrorBody::new(ErrorCode::NotFound, "поста 5 нет");
    assert_eq!(
        serde_json::to_value(&plain).expect("JSON"),
        json!({"code": "not_found", "message": "поста 5 нет"}),
        "без деталей и номера тело такое же, как раньше"
    );
    let mut check = Validator::new();
    check.report(
        "username",
        Violation::new("too_short", "не короче 3 символов"),
    );
    check.report(
        "links[1].url",
        Violation::new("invalid_url", "нужна ссылка"),
    );
    let invalid: Invalid = check.finish().expect_err("две ошибки");
    let full = ErrorBody::invalid(&invalid).with_request_id("8f21");
    assert_eq!(
        serde_json::to_value(&full).expect("JSON"),
        json!({
            "code": "validation",
            "message": "проверь поля",
            "details": [
                {"field": "username", "code": "too_short", "message": "не короче 3 символов"},
                {"field": "links[1].url", "code": "invalid_url", "message": "нужна ссылка"}
            ],
            "request_id": "8f21"
        })
    );
    assert_eq!(full.http_status(), 422);
    assert_eq!(
        full.fields(),
        [("username", "too_short"), ("links[1].url", "invalid_url")]
    );
    let future: Outcome<()> =
        decode(r#"{"error": {"code": "brand_new", "message": "новое", "hint": 1}}"#)
            .expect("незнакомый код не ломает разбор");
    let Outcome::Error(body) = future else {
        panic!("ждали ошибку");
    };
    assert_eq!((body.known(), body.http_status()), (None, 500));
    assert_eq!(
        body.code, "brand_new",
        "строку кода сохраняем, её можно показать"
    );
    let detail: Detail = decode(r#"{"field": "a", "code": "b"}"#).expect("детали без message");
    assert!(detail.message.is_empty());
}

#[test]
fn broken_json_is_400_and_wrong_fields_are_422() {
    #[derive(Debug, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Signup {
        username: String,
        age: u8,
    }
    let fine = parse_body::<Signup>(br#"{"username": "al", "age": 30}"#).expect("форма верна");
    assert_eq!((fine.username.as_str(), fine.age), ("al", 30));
    let syntax = parse_body::<Signup>(b"{not json").expect_err("не JSON");
    assert_eq!(
        (syntax.code.as_str(), syntax.http_status()),
        ("syntax", 400)
    );
    let cut = parse_body::<Signup>(br#"{"username": "al"#).expect_err("оборвано");
    assert_eq!(cut.code, "truncated");
    let missing = parse_body::<Signup>(br#"{"age": 3}"#).expect_err("нет поля");
    assert_eq!(
        (missing.http_status(), missing.fields()),
        (422, vec![("username", "missing")])
    );
    let extra = parse_body::<Signup>(br#"{"username": "a", "age": 3, "admin": true}"#)
        .expect_err("лишнее поле");
    assert_eq!(
        extra.fields(),
        [("admin", "unknown_field")],
        "mass assignment не проходит"
    );
    let typed =
        parse_body::<Signup>(br#"{"username": "a", "age": "hunter2"}"#).expect_err("не тот тип");
    assert_eq!(typed.fields(), [("", "wrong_type")]);
    let shown = serde_json::to_string(&typed).expect("JSON");
    assert!(
        !shown.contains("hunter2"),
        "текст serde с присланным значением в ответ не попадает: {shown}"
    );
}

#[test]
fn email_is_valid_by_construction() {
    let email = Email::try_from("  Alice.Smith+news@Example.COM ").expect("почта");
    assert_eq!(
        email.as_ref(),
        "alice.smith+news@example.com",
        "нормализация до проверки"
    );
    for (raw, code) in [
        ("", "blank"),
        ("alice", "invalid_format"),
        ("alice@", "invalid_format"),
        ("@example.com", "invalid_format"),
        ("alice@example", "invalid_format"),
        ("al ice@example.com", "invalid_format"),
        ("alice@-example.com", "invalid_format"),
    ] {
        let violation: Violation = Email::try_from(raw).expect_err(raw).into();
        assert_eq!(violation.code, code, "{raw:?}");
    }
    let too_long = format!("{}@example.com", "a".repeat(300));
    let violation: Violation = Email::try_from(too_long.as_str())
        .expect_err("длинно")
        .into();
    assert_eq!(violation.code, "too_long");
    let name: Violation = NameError::BadChar(' ').into();
    assert_eq!(name.code, "invalid_char");
    let decoded: Result<Email, _> = serde_json::from_value(json!("bob@example.com"));
    assert!(decoded.is_ok(), "Email разбирается из JSON только валидным");
    let refused: Result<Email, _> = serde_json::from_value(json!("bob"));
    assert!(refused.is_err());
}

#[test]
fn a_patch_tells_missing_from_null() {
    let none: ProfilePatch = decode("{}").expect("пустой PATCH");
    assert_eq!(
        (none.display_name.clone(), none.bio.clone()),
        (Field::Missing, Field::Missing),
        "поле не прислано: не трогать"
    );
    let patch: ProfilePatch = decode(r#"{"display_name": "Алиса", "bio": null}"#).expect("PATCH");
    assert_eq!(patch.display_name, Field::Value("Алиса".to_string()));
    assert_eq!(patch.bio, Field::Null, "null: очистить");
    assert!(
        decode::<ProfilePatch>(r#"{"username": "admin"}"#).is_err(),
        "менять можно только разрешённые поля"
    );
}

#[test]
fn views_carry_new_fields_only_when_they_exist() {
    let user = UserView::named(UserId(1), "alice");
    assert_eq!(
        serde_json::to_value(&user).expect("JSON"),
        json!({"id": "u_1", "username": "alice"})
    );
    let named = UserView {
        display_name: Some("Алиса".to_string()),
        ..user
    };
    assert_eq!(
        serde_json::to_value(&named).expect("JSON")["display_name"],
        json!("Алиса")
    );
    let post = PostView {
        id: PostId(1),
        author: UserId(1),
        author_name: "alice".to_string(),
        text: "привет".to_string(),
        likes: 0,
        tags: vec!["rust".to_string()],
        links: vec![LinkView {
            url: "https://docs.rs".to_string(),
            title: None,
        }],
    };
    let value: Value = serde_json::to_value(&post).expect("JSON");
    assert_eq!(value["tags"], json!(["rust"]));
    assert_eq!(value["links"], json!([{"url": "https://docs.rs"}]));
    let old: PostView = decode(
        r#"{"id": "p_2", "author": "u_1", "author_name": "alice", "text": "старый", "likes": 3}"#,
    )
    .expect("старый формат без тегов читается");
    assert!(old.tags.is_empty() && old.links.is_empty());
}
