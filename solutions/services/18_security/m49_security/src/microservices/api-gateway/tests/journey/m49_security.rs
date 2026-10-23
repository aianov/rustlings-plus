use std::net::SocketAddr;

use api_gateway::Config;
use contracts::{CommentView, PostView};
use social_cli::{Reply, Web};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::support::{Gateway, ok, within};

const FRONT: &str = "https://social.example";

async fn call(
    stack: &Gateway,
    method: &str,
    path: &str,
    body: Option<&str>,
    token: Option<&str>,
) -> Reply {
    within(
        &format!("{method} {path}"),
        Web::new(&stack.http.to_string()).request(method, path, body, token),
    )
    .await
    .expect("шлюз отвечает")
}

async fn upgrade(addr: SocketAddr, origin: Option<&str>) -> String {
    let mut stream = TcpStream::connect(addr).await.expect("порт шлюза");
    let mut head = String::from(
        "GET /ws HTTP/1.1\r\nHost: test\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n",
    );
    if let Some(origin) = origin {
        head.push_str(&format!("Origin: {origin}\r\n"));
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes()).await.expect("запрос");
    let mut buf = vec![0u8; 1024];
    let read = within("ответ на upgrade", stream.read(&mut buf))
        .await
        .expect("ответ");
    String::from_utf8_lossy(&buf[..read])
        .lines()
        .next()
        .unwrap_or_default()
        .to_string()
}

#[tokio::test]
async fn drafts_and_comments_belong_to_their_owners() {
    let stack = Gateway::start(Config::default()).await;
    let (_, mut author) = stack.member("alice").await;
    let (_, mut fan) = stack.member("bob").await;
    let alice = stack.login("alice").await.token;
    let eve = {
        stack.register("eve").await;
        stack.login("eve").await.token
    };
    let saved = call(
        &stack,
        "POST",
        "/drafts",
        Some(r#"{"text": "секретный план"}"#),
        Some(&alice),
    )
    .await;
    assert_eq!(saved.status, 201, "{saved:?}");
    let id = saved
        .body
        .split("\"id\":")
        .nth(1)
        .and_then(|rest| rest.split([',', '}']).next())
        .expect("id черновика")
        .trim()
        .to_string();
    let path = format!("/drafts/{id}");
    let own = call(&stack, "GET", &path, None, Some(&alice)).await;
    assert_eq!(own.status, 200);
    assert!(own.body.contains("секретный план"));
    let peek = call(&stack, "GET", &path, None, Some(&eve)).await;
    assert_eq!(
        peek.status, 404,
        "чужой черновик выглядит несуществующим: {peek:?}"
    );
    let erase = call(&stack, "DELETE", &path, None, Some(&eve)).await;
    assert_eq!(erase.status, 404);
    let anonymous = call(&stack, "GET", &path, None, None).await;
    assert_eq!(anonymous.status, 401);
    assert_eq!(
        call(&stack, "GET", &path, None, Some(&alice)).await.status,
        200,
        "черновик цел"
    );
    let post = ok!(author, "posts.create", r#"{"text": "обсудим"}"#.to_string() => PostView);
    let comment = ok!(
        fan,
        "posts.comment",
        format!(r#"{{"post": "{}", "text": "ерунда"}}"#, post.id) => CommentView
    );
    let comment_path = format!("/comments/{}", comment.id);
    let stranger = call(&stack, "DELETE", &comment_path, None, Some(&eve)).await;
    assert_eq!(
        stranger.status, 403,
        "чужой комментарий не удалить: {stranger:?}"
    );
    let owner = call(&stack, "DELETE", &comment_path, None, Some(&alice)).await;
    assert_eq!(owner.status, 204, "автор поста убирает комментарий под ним");
    let again = call(&stack, "DELETE", &comment_path, None, Some(&alice)).await;
    assert_eq!(again.status, 404);
    stack.stop().await;
}

#[tokio::test]
async fn browsers_from_other_sites_are_kept_out() {
    let stack = Gateway::start(Config {
        cors_origins: vec![FRONT.to_string()],
        ..Config::default()
    })
    .await;
    assert!(
        upgrade(stack.http, Some(FRONT)).await.contains("101"),
        "свой фронт открывает WebSocket"
    );
    let foreign = upgrade(stack.http, Some("https://evil.example")).await;
    assert!(foreign.contains("403"), "чужой Origin отвергнут: {foreign}");
    assert!(
        upgrade(stack.http, None).await.contains("101"),
        "клиент без Origin это не браузер"
    );
    let mut web = Web::new(&stack.http.to_string());
    let preflight = within(
        "предзапрос",
        web.send(
            "OPTIONS",
            "/users/me",
            None,
            &[
                ("Origin", "https://evil.example"),
                ("Access-Control-Request-Method", "PATCH"),
            ],
        ),
    )
    .await
    .expect("шлюз отвечает");
    assert_eq!(
        preflight.header("access-control-allow-origin"),
        None,
        "чужому сайту CORS не разрешает ничего"
    );
    let own = within(
        "предзапрос",
        web.send(
            "OPTIONS",
            "/auth/refresh",
            None,
            &[
                ("Origin", FRONT),
                ("Access-Control-Request-Method", "POST"),
                ("Access-Control-Request-Headers", "x-social-client"),
            ],
        ),
    )
    .await
    .expect("шлюз отвечает");
    assert_eq!(own.header("access-control-allow-origin"), Some(FRONT));
    assert_eq!(own.header("access-control-allow-credentials"), Some("true"));
    assert!(
        own.header("access-control-allow-headers")
            .is_some_and(|list| list.to_ascii_lowercase().contains("x-social-client")),
        "свой заголовок против CSRF разрешён своему фронту"
    );
    stack.stop().await;
}

#[tokio::test]
async fn hostile_input_is_refused_early() {
    let stack = Gateway::start(Config::default()).await;
    let crlf = call(
        &stack,
        "POST",
        "/auth/register",
        Some(r#"{"username": "evil\r\nX-Injected: 1", "password": "correct-horse"}"#),
        None,
    )
    .await;
    assert_eq!(
        crlf.status, 422,
        "ник с переводом строки не проходит: {crlf:?}"
    );
    assert!(crlf.header("x-injected").is_none());
    let mut stream = TcpStream::connect(stack.http).await.expect("порт шлюза");
    stream
        .write_all(
            b"POST /auth/register HTTP/1.1\r\nHost: test\r\nContent-Type: application/json\r\nContent-Length: 2097152\r\n\r\n",
        )
        .await
        .expect("заголовки уходят");
    let mut buf = vec![0u8; 512];
    let read = within("ответ на огромное тело", stream.read(&mut buf))
        .await
        .expect("ответ");
    let status = String::from_utf8_lossy(&buf[..read]).to_string();
    assert!(
        status.starts_with("HTTP/1.1 413"),
        "тело больше лимита отвергнуто по заголовку, до чтения и разбора: {status}"
    );
    let (_, mut device) = stack.member("alice").await;
    let tags: Vec<String> = (0..50).map(|n| format!("\"t{n}\"")).collect();
    let many = device
        .fails(
            "posts.create",
            &format!(r#"{{"text": "теги", "tags": [{}]}}"#, tags.join(", ")),
        )
        .await;
    assert_eq!(
        many.code, "validation",
        "длина коллекций ограничена: {many:?}"
    );
    stack.stop().await;
}
