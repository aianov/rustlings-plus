use std::net::SocketAddr;

use api_gateway::Config;
use contracts::{PostView, Push, UserView};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::support::{Gateway, ok, within};

const APP: &str = "https://app.example";

async fn handshake(addr: SocketAddr, path: &str, extra: &[(&str, &str)]) -> String {
    let mut stream = within("подключение", TcpStream::connect(addr))
        .await
        .expect("HTTP-порт открыт");
    let mut request = format!(
        "GET {path} HTTP/1.1\r\nHost: test\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n"
    );
    for (name, value) in extra {
        request.push_str(&format!("{name}: {value}\r\n"));
    }
    request.push_str("\r\n");
    stream
        .write_all(request.as_bytes())
        .await
        .expect("запрос уходит");
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        let n = within("ответ на рукопожатие", stream.read(&mut byte))
            .await
            .expect("ответ читается");
        if n == 0 {
            break;
        }
        head.push(byte[0]);
    }
    String::from_utf8_lossy(&head).into_owned()
}

fn status(head: &str) -> u16 {
    head.split(' ')
        .nth(1)
        .and_then(|code| code.parse().ok())
        .unwrap_or_default()
}

#[tokio::test]
async fn token_in_the_url_introduces_the_socket() {
    let gateway = Gateway::start(Config::default()).await;
    let alice = gateway.register("alice").await;
    let login = gateway.login("alice").await;
    let mut peer = gateway.peer_with(&login.token).await;
    let post = ok!(peer, "posts.create", r#"{"text": "Привет из WebSocket"}"# => PostView);
    assert_eq!(
        post.author, alice.id,
        "по токену из ?token= сокет сразу представлен: hello не нужен"
    );
    let twice = peer
        .fails(
            "session.hello",
            &format!(r#"{{"token": "{}"}}"#, login.token),
        )
        .await;
    assert_eq!(twice.code, "conflict", "сокет уже представлен");
    let raw = handshake(
        gateway.http,
        "/ws?token=00000000000000000000000000000000",
        &[],
    )
    .await;
    assert_eq!(
        status(&raw),
        401,
        "чужой токен отклоняется до 101, обычным HTTP-ответом: {raw}"
    );
    gateway.stop().await;
}

#[tokio::test]
async fn handshake_follows_the_rfc() {
    let gateway = Gateway::start(Config {
        cors_origins: vec![APP.to_string()],
        ..Config::default()
    })
    .await;
    let ok = handshake(gateway.http, "/ws", &[]).await;
    assert_eq!(
        status(&ok),
        101,
        "без Origin (не браузер) сокет открывается: {ok}"
    );
    assert!(
        ok.contains("Sec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo="),
        "Accept считается по RFC 6455: {ok}"
    );
    let friend = handshake(gateway.http, "/ws", &[("Origin", APP)]).await;
    assert_eq!(status(&friend), 101, "свой фронт открывает сокет");
    let evil = handshake(gateway.http, "/ws", &[("Origin", "https://evil.example")]).await;
    assert_eq!(
        status(&evil),
        403,
        "чужая страница не откроет сокет от имени пользователя (CSWSH): {evil}"
    );
    let old = handshake(gateway.http, "/ws", &[("Sec-WebSocket-Version", "8")]).await;
    assert_eq!(
        status(&old),
        426,
        "версия не 13 (здесь их две) это 426 с подсказкой: {old}"
    );
    let mut plain = TcpStream::connect(gateway.http).await.expect("порт открыт");
    plain
        .write_all(b"GET /ws HTTP/1.1\r\nHost: t\r\nConnection: close\r\n\r\n")
        .await
        .expect("запрос уходит");
    let mut answer = String::new();
    within("ответ", plain.read_to_string(&mut answer))
        .await
        .expect("ответ читается");
    assert!(
        answer.starts_with("HTTP/1.1 400 "),
        "GET /ws без Upgrade это 400: {answer}"
    );
    gateway.stop().await;
}

#[tokio::test]
async fn everything_works_over_frames() {
    let gateway = Gateway::start(Config::default()).await;
    let (alice, mut phone) = gateway.member("alice").await;
    let (bob, mut other) = gateway.member("bob").await;
    let _ = ok!(other, "users.follow", format!(r#"{{"target": "{}"}}"#, alice.id) => contracts::Changed);
    assert_eq!(
        phone.push().await,
        Push::Followed {
            to: alice.id,
            by: bob.id
        },
        "push приходит текстовым фреймом"
    );
    let found = ok!(phone, "users.find", r#"{"username": "bob"}"# => UserView);
    assert_eq!(
        found.id, bob.id,
        "запрос и ответ это текстовые фреймы с тем же конвертом"
    );
    let health = gateway.http("GET", "/health", None).await;
    assert_eq!(
        health.status, 200,
        "обычный HTTP на том же порту работает рядом с сокетами"
    );
    gateway.stop().await;
}

#[tokio::test]
async fn binary_frames_are_not_envelopes() {
    let gateway = Gateway::start(Config::default()).await;
    let mut peer = gateway.peer().await;
    peer.binary(&[1, 2, 3]).await;
    assert!(peer.closed().await, "двоичный фрейм закрывает сокет");
    assert_eq!(
        peer.close_code(),
        Some(1003),
        "конверты ходят только текстом: двоичные данные это 1003"
    );
    gateway.stop().await;
}
