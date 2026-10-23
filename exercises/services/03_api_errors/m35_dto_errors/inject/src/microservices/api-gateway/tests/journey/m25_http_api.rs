use std::collections::HashSet;

use api_gateway::Config;
use contracts::{Changed, ErrorBody, Outcome, PostView, UserView, decode, is_token};
use tokio::net::TcpStream;

use crate::support::{Gateway, Http, ok};

fn error_of(answer: &Http) -> ErrorBody {
    match decode::<Outcome<()>>(&answer.body) {
        Ok(Outcome::Error(body)) => body,
        other => panic!("ждали {{\"error\": {{code, message}}}}, а пришло {other:?}: {answer:?}"),
    }
}

#[tokio::test]
async fn register_and_login_over_http() {
    let gateway = Gateway::start(Config::default()).await;
    let alice = gateway.register("Alice").await;
    assert_eq!(
        alice.username, "alice",
        "POST /auth/register отвечает 201 и UserView, ник нормализован"
    );
    let taken = gateway
        .auth_http("POST", "/auth/register", Some(r#"{"username": "alice"}"#))
        .await;
    assert_eq!(
        (taken.status, error_of(&taken).code.as_str()),
        (409, "conflict"),
        "занятый ник это 409 conflict: {taken:?}"
    );
    let crooked = gateway
        .auth_http("POST", "/auth/register", Some(r#"{"username": "a b"}"#))
        .await;
    assert_eq!(
        (crooked.status, error_of(&crooked).fields()),
        (422, vec![("username", "invalid_char")]),
        "JSON разобран, а ник не прошёл проверку: 422 с полем username: {crooked:?}"
    );
    let broken = gateway
        .auth_http("POST", "/auth/register", Some("{not json"))
        .await;
    assert_eq!(broken.status, 400, "кривой JSON это 400: {broken:?}");
    let unknown = gateway
        .auth_http("POST", "/auth/login", Some(r#"{"username": "bob"}"#))
        .await;
    assert_eq!(
        (unknown.status, error_of(&unknown).code.as_str()),
        (401, "unauthorized"),
        "входить можно только существующим ником: {unknown:?}"
    );
    let first = gateway.login("alice").await;
    let second = gateway.login("ALICE").await;
    assert_eq!(first.user, alice, "вход отвечает токеном и пользователем");
    assert!(
        is_token(&first.token) && is_token(&second.token),
        "токен это 32 шестнадцатеричных знака (128 бит): {:?}",
        first.token
    );
    assert_ne!(first.token, second.token, "каждый вход это новый токен");
    gateway.stop().await;
}

#[tokio::test]
async fn tokens_do_not_repeat() {
    let gateway = Gateway::start(Config::default()).await;
    gateway.register("alice").await;
    let mut seen = HashSet::new();
    for _ in 0..40 {
        let login = gateway.login("alice").await;
        assert!(
            seen.insert(login.token.clone()),
            "токен {} повторился",
            login.token
        );
    }
    let digits: HashSet<char> = seen.iter().flat_map(|token| token.chars()).collect();
    assert_eq!(
        digits.len(),
        16,
        "в 40 токенах по 32 знака встречаются все 16 шестнадцатеричных цифр, иначе генератор не случайный"
    );
    gateway.stop().await;
}

#[tokio::test]
async fn profiles_and_posts_by_link() {
    let gateway = Gateway::start(Config::default()).await;
    let (alice, mut peer) = gateway.member("alice").await;
    let post = ok!(peer, "posts.create", r#"{"text": "Ссылка на меня"}"# => PostView);
    let gone = ok!(peer, "posts.create", r#"{"text": "Удалю"}"# => PostView);
    let _ = ok!(peer, "posts.delete", format!(r#"{{"post": "{}"}}"#, gone.id) => Changed);

    let profile = gateway.http("GET", "/users/alice", None).await;
    assert_eq!(profile.status, 200, "GET /users/:username: {profile:?}");
    let user: UserView =
        decode(&profile.body).unwrap_or_else(|e| panic!("профиль разбирается ({e}): {profile:?}"));
    assert_eq!(user, alice, "профиль по нику");
    assert_eq!(
        profile.header("content-type"),
        Some("application/json"),
        "JSON-ответ с Content-Type"
    );
    let nobody = gateway.http("GET", "/users/nobody", None).await;
    assert_eq!(
        (nobody.status, error_of(&nobody).code.as_str()),
        (404, "not_found")
    );
    let bad = gateway.http("GET", "/users/Bad!", None).await;
    assert_eq!(bad.status, 400, "кривой ник в пути это 400: {bad:?}");

    for path in [
        format!("/posts/{}", post.id.0),
        format!("/posts/{}", post.id),
    ] {
        let answer = gateway.http("GET", &path, None).await;
        assert_eq!(answer.status, 200, "GET {path}: {answer:?}");
        let view: PostView =
            decode(&answer.body).unwrap_or_else(|e| panic!("пост разбирается ({e}): {answer:?}"));
        assert_eq!(view, post, "пост по ссылке, id числом или p_N");
    }
    let deleted = gateway
        .http("GET", &format!("/posts/{}", gone.id.0), None)
        .await;
    assert_eq!(
        deleted.status, 404,
        "удалённый пост по ссылке не виден: {deleted:?}"
    );
    let odd = gateway.http("GET", "/posts/abc", None).await;
    assert_eq!(odd.status, 400, "не id поста это 400: {odd:?}");
    gateway.stop().await;
}

#[tokio::test]
async fn health_head_and_unknown_routes() {
    let gateway = Gateway::start(Config::default()).await;
    let health = gateway.http("GET", "/health", None).await;
    assert_eq!(
        (health.status, health.body.as_str()),
        (200, "ok"),
        "GET /health"
    );
    let head = gateway.http("HEAD", "/health", None).await;
    assert_eq!(
        (
            head.status,
            head.body.as_str(),
            head.header("content-length")
        ),
        (200, "", Some("2")),
        "HEAD: заголовки как у GET и пустое тело"
    );
    let wrong = gateway.http("POST", "/health", Some("{}")).await;
    assert_eq!(
        (wrong.status, wrong.header("allow")),
        (405, Some("GET, HEAD")),
        "путь есть, метода нет: 405 с Allow"
    );
    let missing = gateway.http("GET", "/nope", None).await;
    assert_eq!(missing.status, 404, "неизвестный адрес 404");
    let http = gateway.http;
    gateway.stop().await;
    assert!(
        TcpStream::connect(http).await.is_err(),
        "после остановки HTTP-порт закрыт"
    );
}
