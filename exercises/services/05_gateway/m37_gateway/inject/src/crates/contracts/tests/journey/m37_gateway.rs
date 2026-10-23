use contracts::{
    Envelope, ErrorCode, Map, PostId, ProfileView, StatsView, UserId, UserView, Value, decode,
    encode, read_data, to_data,
};
use serde::Deserialize;
use serde_json::json;

#[derive(Debug, Deserialize, PartialEq)]
struct Like {
    post: PostId,
}

#[test]
fn typed_data_errors_name_fields_but_not_values() {
    let missing = read_data::<Like>("posts.like", json!({})).expect_err("поля нет");
    assert_eq!(
        (missing.code.as_str(), missing.fields()),
        ("bad_data", vec![("post", "missing")]),
        "нехватку поля называем: имя поля из схемы"
    );
    let wrong = read_data::<Like>("posts.like", json!({"post": "пароль-123"})).expect_err("не id");
    assert!(
        wrong.code == "bad_data" && !format!("{wrong:?}").contains("пароль"),
        "текст serde цитирует присланное, в ответ он не идёт: {wrong:?}"
    );
    assert!(wrong.message.contains("posts.like"), "{wrong:?}");
    assert_eq!(
        read_data::<Like>("posts.like", json!({"post": "p_3", "user": "u_1"})),
        Ok(Like { post: PostId(3) }),
        "лишнее поле user не мешает: его проверяет шлюз, а не тип данных"
    );
    assert_eq!(
        to_data(&UserView::named(UserId(1), "alice")),
        Ok(json!({"id": "u_1", "username": "alice"}))
    );
}

#[test]
fn the_frame_carries_optional_meta() {
    let plain = Envelope::new(1, "users.find", json!({"username": "bob"}));
    assert_eq!(
        encode(&plain).map(|text| text.contains("meta")),
        Ok(false),
        "пустой meta на провод не пишется: старые клиенты его не видят"
    );
    let traced = plain.clone().with_meta("x-trace-id", "trace-7");
    let back: Envelope = decode(&encode(&traced).expect("пишется")).expect("читается");
    assert_eq!(back.meta.get("x-trace-id"), Some(&json!("trace-7")));
    let old: Envelope =
        decode(r#"{"id": 2, "service": "users", "method": "users.find", "data": {}}"#)
            .expect("кадр без meta тоже разбирается");
    assert_eq!(old.meta, Map::new());
}

#[test]
fn a_card_marks_missing_parts_with_null() {
    let card = ProfileView {
        user: UserView::named(UserId(1), "alice"),
        posts: None,
        stats: Some(StatsView {
            followers: 2,
            following: 1,
        }),
        partial: vec![String::from("posts")],
    };
    let wire: Value = serde_json::from_str(&encode(&card).expect("пишется")).expect("JSON");
    assert_eq!(
        wire,
        json!({
            "user": {"id": "u_1", "username": "alice"},
            "posts": null,
            "stats": {"followers": 2, "following": 1},
            "partial": ["posts"]
        }),
        "недошедшая часть это null, а её имя в partial: клиент нарисует остальное"
    );
}

#[test]
fn exhausted_is_temporary_and_429() {
    assert_eq!(
        ErrorCode::from_wire("exhausted"),
        Some(ErrorCode::Exhausted)
    );
    assert_eq!(ErrorCode::Exhausted.http_status(), 429);
    assert!(ErrorCode::Exhausted.is_temporary() && ErrorCode::Exhausted.is_client_fault());
    assert_eq!(ErrorCode::for_status(429), ErrorCode::Exhausted);
}
