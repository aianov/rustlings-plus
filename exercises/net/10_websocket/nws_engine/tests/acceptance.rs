use std::future::Future;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bytes::BytesMut;
use http_engine::{Method, Request, Response, Router, ServerConfig, handler, serve_with};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::timeout;
use websocket_engine::{
    Decoder, Frame, MaskGen, Message, Opcode, Role, WsConfig, WsError, WsStream, accept_key, base64_decode, base64_encode, connect, encode_close, encode_frame,
    parse_close,
};

const RFC_KEY: &str = "dGhlIHNhbXBsZSBub25jZQ==";
const RFC_ACCEPT: &str = "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=";

async fn within<T>(what: &str, fut: impl Future<Output = T>) -> T {
    match timeout(Duration::from_secs(5), fut).await {
        Ok(v) => v,
        Err(_) => panic!("{what}: за 5 секунд ничего не произошло"),
    }
}

type Log = Arc<Mutex<Vec<String>>>;

async fn echo(mut ws: WsStream<TcpStream>) {
    while let Ok(Some(message)) = ws.recv().await {
        if ws.send(message).await.is_err() {
            break;
        }
    }
}

async fn collect(mut ws: WsStream<TcpStream>, log: Log) {
    loop {
        match ws.recv().await {
            Ok(Some(Message::Text(t))) => log.lock().unwrap().push(format!("text:{t}")),
            Ok(Some(Message::Binary(b))) => log.lock().unwrap().push(format!("bin:{}", b.len())),
            Ok(None) => {
                log.lock().unwrap().push(format!("eof:{:?}", ws.close_code()));
                return;
            }
            Err(e) => {
                log.lock().unwrap().push(format!("err:{e}"));
                return;
            }
        }
    }
}

async fn start(config: WsConfig) -> (SocketAddr, Log) {
    let log: Log = Arc::new(Mutex::new(Vec::new()));
    let ws_route = |kind: &'static str, config: WsConfig, log: Log| {
        handler(move |request: Request| {
            let config = config.clone();
            let log = log.clone();
            async move {
                upgrade_with(kind, &request, &config, log)
            }
        })
    };
    let router = Router::new()
        .route(Method::Get, "/echo", ws_route("echo", config.clone(), log.clone()))
        .route(Method::Post, "/echo", ws_route("echo", config.clone(), log.clone()))
        .route(Method::Get, "/collect", ws_route("collect", config.clone(), log.clone()))
        .route(Method::Get, "/closer", ws_route("closer", config.clone(), log.clone()))
        .route(Method::Get, "/silent", ws_route("silent", config.clone(), log.clone()))
        .route(Method::Get, "/health", handler(|_request: Request| async { Response::text(200, "ok") }));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = serve_with(listener, router.into_handler(), ServerConfig::default(), std::future::pending()).await;
    });
    (addr, log)
}

fn upgrade_with(kind: &'static str, request: &Request, config: &WsConfig, log: Log) -> Response {
    match kind {
        "echo" => websocket_engine::upgrade(request, config, echo),
        "collect" => websocket_engine::upgrade(request, config, move |ws| collect(ws, log)),
        "closer" => websocket_engine::upgrade(request, config, |mut ws| async move {
            let _ = ws.send(Message::Text("hello".to_string())).await;
            let _ = ws.close(1000, "bye").await;
        }),
        _ => websocket_engine::upgrade(request, config, move |mut ws| async move {
            let started = Instant::now();
            let _ = ws.close(1000, "silence").await;
            log.lock().unwrap().push(format!("close_ms:{}", started.elapsed().as_millis()));
        }),
    }
}

async fn wait_log(log: &Log, needle: &str) -> String {
    within(&format!("в журнале сервера нет записи с «{needle}»"), async {
        loop {
            if let Some(line) = log.lock().unwrap().iter().find(|l| l.contains(needle)) {
                return line.clone();
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
}

fn client_frame(b0: u8, payload: &[u8], key: [u8; 4]) -> Vec<u8> {
    let mut out = vec![b0];
    match payload.len() {
        n if n < 126 => out.push(0x80 | n as u8),
        n if n < 65536 => {
            out.push(0x80 | 126);
            out.extend_from_slice(&(n as u16).to_be_bytes());
        }
        n => {
            out.push(0x80 | 127);
            out.extend_from_slice(&(n as u64).to_be_bytes());
        }
    }
    out.extend_from_slice(&key);
    out.extend(payload.iter().enumerate().map(|(i, b)| b ^ key[i % 4]));
    out
}

struct Raw {
    stream: TcpStream,
    buf: Vec<u8>,
}

impl Raw {
    async fn connect(addr: SocketAddr) -> Raw {
        let stream = TcpStream::connect(addr).await.unwrap();
        stream.set_nodelay(true).unwrap();
        Raw { stream, buf: Vec::new() }
    }

    async fn send(&mut self, bytes: &[u8]) {
        self.stream.write_all(bytes).await.unwrap();
        self.stream.flush().await.unwrap();
    }

    async fn fill(&mut self) -> bool {
        let mut chunk = [0u8; 8192];
        match within("чтение из сокета", self.stream.read(&mut chunk)).await {
            Ok(0) | Err(_) => false,
            Ok(n) => {
                self.buf.extend_from_slice(&chunk[..n]);
                true
            }
        }
    }

    async fn head(&mut self) -> String {
        loop {
            if let Some(pos) = self.buf.windows(4).position(|w| w == b"\r\n\r\n") {
                let head = String::from_utf8_lossy(&self.buf[..pos + 4]).into_owned();
                self.buf.drain(..pos + 4);
                return head;
            }
            assert!(self.fill().await, "сервер закрыл соединение, не договорив ответ");
        }
    }

    async fn request(&mut self, text: &str) -> String {
        self.send(text.as_bytes()).await;
        self.head().await
    }

    async fn handshake(&mut self, path: &str) -> String {
        let req = upgrade_request(path, &[]);
        self.request(&req).await
    }

    async fn frame(&mut self) -> Option<(u8, Vec<u8>)> {
        loop {
            if self.buf.len() >= 2 {
                let b0 = self.buf[0];
                let b1 = self.buf[1];
                assert_eq!(b1 & 0x80, 0, "сервер прислал замаскированный фрейм: серверные фреймы маскировать нельзя");
                let (len, header) = match b1 & 0x7f {
                    126 if self.buf.len() >= 4 => {
                        let n = usize::from(u16::from_be_bytes([self.buf[2], self.buf[3]]));
                        assert!(n >= 126, "длина {n} записана в двух байтах, хотя влезает в семь бит: нужна кратчайшая форма");
                        (n, 4)
                    }
                    127 if self.buf.len() >= 10 => {
                        let n = u64::from_be_bytes(self.buf[2..10].try_into().unwrap()) as usize;
                        assert!(n >= 65536, "длина {n} записана в восьми байтах, хотя влезает в два: нужна кратчайшая форма");
                        (n, 10)
                    }
                    126 | 127 => (usize::MAX, 0),
                    n => (usize::from(n), 2),
                };
                if len != usize::MAX && self.buf.len() >= header + len {
                    let payload = self.buf[header..header + len].to_vec();
                    self.buf.drain(..header + len);
                    return Some((b0, payload));
                }
            }
            if !self.fill().await {
                return None;
            }
        }
    }

    async fn expect_close(&mut self, code: u16) {
        let (b0, payload) = self.frame().await.unwrap_or_else(|| panic!("ждали Close с кодом {code}, а сервер просто закрыл соединение"));
        assert_eq!(b0, 0x88, "ждали фрейм Close (0x88), а пришло {b0:#04x}");
        assert!(payload.len() >= 2, "в Close нет кода: {payload:?}");
        assert_eq!(u16::from_be_bytes([payload[0], payload[1]]), code, "код закрытия");
    }

    async fn expect_eof(&mut self) {
        let mut rest = Vec::new();
        let r = within("ожидание закрытия сокета сервером", self.stream.read_to_end(&mut rest)).await;
        assert!(r.is_ok() || rest.is_empty(), "после Close сервер должен закрыть соединение");
    }
}

fn upgrade_request(path: &str, extra: &[(&str, &str)]) -> String {
    let mut out = format!("GET {path} HTTP/1.1\r\nHost: test\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {RFC_KEY}\r\nSec-WebSocket-Version: 13\r\n");
    for (name, value) in extra {
        out.push_str(&format!("{name}: {value}\r\n"));
    }
    out.push_str("\r\n");
    out
}

fn status_of(head: &str) -> u16 {
    head.split(' ').nth(1).and_then(|s| s.parse().ok()).unwrap_or(0)
}

fn header_of(head: &str, name: &str) -> Vec<String> {
    head.split("\r\n")
        .skip(1)
        .filter_map(|l| l.split_once(':'))
        .filter(|(n, _)| n.trim().eq_ignore_ascii_case(name))
        .map(|(_, v)| v.trim().to_string())
        .collect()
}

const KEY: [u8; 4] = [0x11, 0x22, 0x33, 0x44];

// ---------------------------------------------------------------- base64 и ключ

#[test]
fn base64_vectors_and_strictness() {
    for (plain, enc) in [("", ""), ("f", "Zg=="), ("fo", "Zm8="), ("foo", "Zm9v"), ("foob", "Zm9vYg=="), ("fooba", "Zm9vYmE="), ("foobar", "Zm9vYmFy")] {
        assert_eq!(base64_encode(plain.as_bytes()), enc);
        assert_eq!(base64_decode(enc).unwrap(), plain.as_bytes());
    }
    for bad in ["Zg=", "Z===", "Zg=a", "Zh==", "Zm 9", "Zm-v", "=Zg=", "Zg==Zg=="] {
        assert!(base64_decode(bad).is_err(), "{bad:?} должен быть отвергнут");
    }
}

#[test]
fn accept_key_from_rfc() {
    assert_eq!(accept_key(RFC_KEY), RFC_ACCEPT);
    assert_eq!(accept_key("x3JJHMbDL1EzLkh9GBhXDw=="), "HSmrc0sMlYUkAGmm5OPpG2HaGWk=");
}

// ---------------------------------------------------------------- рукопожатие

#[tokio::test]
async fn handshake_answers_101_with_accept() {
    let (addr, _log) = start(WsConfig::default()).await;
    let mut raw = Raw::connect(addr).await;
    let head = raw.handshake("/echo").await;
    assert_eq!(status_of(&head), 101, "ответ: {head:?}");
    assert_eq!(header_of(&head, "sec-websocket-accept"), vec![RFC_ACCEPT]);
    assert!(header_of(&head, "upgrade").iter().any(|v| v.eq_ignore_ascii_case("websocket")));
    assert!(header_of(&head, "connection").iter().any(|v| v.eq_ignore_ascii_case("upgrade")));
    assert!(header_of(&head, "content-length").is_empty(), "у 101 нет тела и нет Content-Length");
}

#[tokio::test]
async fn firefox_style_connection_header_is_accepted() {
    let (addr, _log) = start(WsConfig::default()).await;
    let mut raw = Raw::connect(addr).await;
    let req = format!("GET /echo HTTP/1.1\r\nHost: t\r\nUpgrade: websocket\r\nConnection: keep-alive, Upgrade\r\nSec-WebSocket-Key: {RFC_KEY}\r\nSec-WebSocket-Version: 13\r\n\r\n");
    let head = raw.request(&req).await;
    assert_eq!(status_of(&head), 101, "Firefox шлёт список токенов в Connection: {head:?}");
}

#[tokio::test]
async fn bad_upgrade_requests_get_proper_statuses() {
    let (addr, _log) = start(WsConfig::default()).await;
    let cases: Vec<(String, u16, &str)> = vec![
        (format!("POST /echo HTTP/1.1\r\nHost: t\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {RFC_KEY}\r\nSec-WebSocket-Version: 13\r\nContent-Length: 0\r\n\r\n"), 405, "POST"),
        ("GET /echo HTTP/1.1\r\nHost: t\r\n\r\n".to_string(), 400, "обычный GET без Upgrade"),
        ("GET /echo HTTP/1.1\r\nHost: t\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Version: 13\r\n\r\n".to_string(), 400, "нет ключа"),
        ("GET /echo HTTP/1.1\r\nHost: t\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: AAAAAAAAAAAAAAAAAAAAAAAA\r\nSec-WebSocket-Version: 13\r\n\r\n".to_string(), 400, "ключ из 24 символов, но не 16 байт"),
        (format!("GET /echo HTTP/1.1\r\nHost: t\r\nUpgrade: websocket\r\nSec-WebSocket-Key: {RFC_KEY}\r\nSec-WebSocket-Version: 13\r\n\r\n"), 400, "нет Connection: Upgrade"),
        (format!("GET /echo HTTP/1.1\r\nHost: t\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {RFC_KEY}\r\nSec-WebSocket-Version: 12\r\n\r\n"), 426, "версия 12"),
        (format!("GET /echo HTTP/1.1\r\nHost: t\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {RFC_KEY}\r\n\r\n"), 426, "версии нет"),
    ];
    for (req, expected, what) in cases {
        let mut raw = Raw::connect(addr).await;
        let head = raw.request(&req).await;
        assert_eq!(status_of(&head), expected, "{what}: {head:?}");
        if expected == 426 {
            assert_eq!(header_of(&head, "sec-websocket-version"), vec!["13"], "426 обязан подсказать версию");
        }
        if expected == 405 {
            assert!(header_of(&head, "allow").iter().any(|v| v.contains("GET")), "405 обязан подсказать Allow");
        }
    }
}

#[tokio::test]
async fn origin_policy_protects_against_foreign_pages() {
    let config = WsConfig { allowed_origins: vec!["https://app.example".to_string()], allow_missing_origin: false, ..WsConfig::default() };
    let (addr, _log) = start(config).await;
    for (origin, expected) in [
        (Some("https://app.example"), 101),
        (Some("https://evil.example"), 403),
        (Some("https://app.example.evil.com"), 403),
        (Some("http://app.example"), 403),
        (Some("null"), 403),
        (None, 403),
    ] {
        let mut raw = Raw::connect(addr).await;
        let extra: Vec<(&str, &str)> = origin.map(|o| vec![("Origin", o)]).unwrap_or_default();
        let head = raw.request(&upgrade_request("/echo", &extra)).await;
        assert_eq!(status_of(&head), expected, "Origin {origin:?}: {head:?}");
    }
}

#[tokio::test]
async fn missing_origin_is_fine_when_the_policy_allows_it() {
    let config = WsConfig { allowed_origins: vec!["https://app.example".to_string()], allow_missing_origin: true, ..WsConfig::default() };
    let (addr, _log) = start(config).await;
    let mut raw = Raw::connect(addr).await;
    assert_eq!(status_of(&raw.handshake("/echo").await), 101);
}

// ---------------------------------------------------------------- обмен сообщениями

#[tokio::test]
async fn own_client_talks_to_own_server() {
    let (addr, _log) = start(WsConfig::default()).await;
    let mut ws = within("подключение", connect(addr, "test", "/echo", None, &WsConfig::default())).await.expect("клиент должен подключиться");
    for text in ["привет", "", "😀 emoji", &"x".repeat(70_000)] {
        ws.send(Message::Text(text.to_string())).await.unwrap();
        assert_eq!(within("эхо", ws.recv()).await.unwrap(), Some(Message::Text(text.to_string())));
    }
    let binary: Vec<u8> = (0..=255u8).cycle().take(200_000).collect();
    ws.send(Message::Binary(binary.clone())).await.unwrap();
    assert_eq!(within("эхо", ws.recv()).await.unwrap(), Some(Message::Binary(binary)), "двоичное сообщение на 200 КБ");
}

#[tokio::test]
async fn first_frame_may_arrive_together_with_the_handshake() {
    let (addr, _log) = start(WsConfig::default()).await;
    let mut raw = Raw::connect(addr).await;
    let mut all = upgrade_request("/echo", &[]).into_bytes();
    all.extend(client_frame(0x81, b"early bird", KEY));
    raw.send(&all).await;
    let head = raw.head().await;
    assert_eq!(status_of(&head), 101);
    let (b0, payload) = raw.frame().await.expect("эхо первого сообщения");
    assert_eq!((b0, payload.as_slice()), (0x81, &b"early bird"[..]), "сообщение, пришедшее вместе с рукопожатием, не должно потеряться");
}

#[tokio::test]
async fn server_frames_are_not_masked() {
    let (addr, _log) = start(WsConfig::default()).await;
    let mut raw = Raw::connect(addr).await;
    raw.handshake("/echo").await;
    for size in [0usize, 5, 125, 126, 65535, 65536] {
        let payload = vec![b'z'; size];
        raw.send(&client_frame(0x82, &payload, KEY)).await;
        let (b0, got) = raw.frame().await.expect("эхо");
        assert_eq!(b0, 0x82);
        assert_eq!(got.len(), size);
    }
}

#[tokio::test]
async fn fragments_with_a_ping_in_the_middle() {
    let (addr, _log) = start(WsConfig::default()).await;
    let mut raw = Raw::connect(addr).await;
    raw.handshake("/echo").await;
    raw.send(&client_frame(0x01, b"Hel", KEY)).await;
    raw.send(&client_frame(0x89, b"p", KEY)).await;
    raw.send(&client_frame(0x80, b"lo", KEY)).await;
    let (b0, payload) = raw.frame().await.expect("pong");
    assert_eq!((b0, payload.as_slice()), (0x8a, &b"p"[..]), "ping посреди сообщения получает pong сразу");
    let (b0, payload) = raw.frame().await.expect("сообщение");
    assert_eq!((b0, payload.as_slice()), (0x81, &b"Hello"[..]), "сообщение собрано из трёх фрагментов");
}

#[tokio::test]
async fn emoji_split_across_fragments_survives() {
    let (addr, _log) = start(WsConfig::default()).await;
    let mut raw = Raw::connect(addr).await;
    raw.handshake("/echo").await;
    let bytes = "😀".as_bytes();
    for (i, b) in bytes.iter().enumerate() {
        let b0 = if i == 0 { 0x01 } else if i + 1 == bytes.len() { 0x80 } else { 0x00 };
        raw.send(&client_frame(b0, &[*b], KEY)).await;
    }
    let (b0, payload) = raw.frame().await.expect("эхо");
    assert_eq!((b0, payload.as_slice()), (0x81, "😀".as_bytes()), "эмодзи, разрезанный по байтам, это корректный текст");
}

#[tokio::test]
async fn application_does_not_have_to_answer_pings() {
    let (addr, log) = start(WsConfig::default()).await;
    let mut raw = Raw::connect(addr).await;
    raw.handshake("/collect").await;
    raw.send(&client_frame(0x89, b"abc", KEY)).await;
    let (b0, payload) = raw.frame().await.expect("pong");
    assert_eq!((b0, payload.as_slice()), (0x8a, &b"abc"[..]), "pong отправляет сам WsStream, пока приложение ждёт в recv");
    raw.send(&client_frame(0x8a, b"unsolicited", KEY)).await;
    raw.send(&client_frame(0x81, b"after", KEY)).await;
    wait_log(&log, "text:after").await;
    assert!(!log.lock().unwrap().iter().any(|l| l.contains("err")), "pong без ping не ошибка");
}

// ---------------------------------------------------------------- нарушения протокола

#[tokio::test]
async fn unmasked_client_frame_is_a_protocol_error() {
    let (addr, log) = start(WsConfig::default()).await;
    let mut raw = Raw::connect(addr).await;
    raw.handshake("/collect").await;
    raw.send(b"\x81\x05hello").await;
    raw.expect_close(1002).await;
    wait_log(&log, "err").await;
}

#[tokio::test]
async fn reserved_bits_and_unknown_opcodes_are_protocol_errors() {
    let (addr, _log) = start(WsConfig::default()).await;
    for b0 in [0xc1u8, 0x91, 0x83, 0x8b] {
        let mut raw = Raw::connect(addr).await;
        raw.handshake("/echo").await;
        raw.send(&client_frame(b0, b"x", KEY)).await;
        raw.expect_close(1002).await;
    }
}

#[tokio::test]
async fn bad_control_frames_are_protocol_errors() {
    let (addr, _log) = start(WsConfig::default()).await;
    let mut raw = Raw::connect(addr).await;
    raw.handshake("/echo").await;
    raw.send(&client_frame(0x09, b"x", KEY)).await;
    raw.expect_close(1002).await;
    let mut raw = Raw::connect(addr).await;
    raw.handshake("/echo").await;
    raw.send(&client_frame(0x89, &[0u8; 126], KEY)).await;
    raw.expect_close(1002).await;
    let mut raw = Raw::connect(addr).await;
    raw.handshake("/echo").await;
    raw.send(&client_frame(0x80, b"x", KEY)).await;
    raw.expect_close(1002).await;
    let mut raw = Raw::connect(addr).await;
    raw.handshake("/echo").await;
    raw.send(&client_frame(0x01, b"a", KEY)).await;
    raw.send(&client_frame(0x01, b"b", KEY)).await;
    raw.expect_close(1002).await;
}

#[tokio::test]
async fn invalid_utf8_closes_with_1007_as_early_as_possible() {
    let (addr, _log) = start(WsConfig::default()).await;
    let mut raw = Raw::connect(addr).await;
    raw.handshake("/echo").await;
    raw.send(&client_frame(0x81, &[0x61, 0xff], KEY)).await;
    raw.expect_close(1007).await;
    let mut raw = Raw::connect(addr).await;
    raw.handshake("/echo").await;
    raw.send(&client_frame(0x01, &[0xc0, 0x80], KEY)).await;
    raw.expect_close(1007).await;
}

#[tokio::test]
async fn too_big_messages_close_with_1009() {
    let config = WsConfig { max_message: 1000, ..WsConfig::default() };
    let (addr, _log) = start(config).await;
    let mut raw = Raw::connect(addr).await;
    raw.handshake("/echo").await;
    raw.send(&client_frame(0x02, &[0u8; 600], KEY)).await;
    raw.send(&client_frame(0x80, &[0u8; 600], KEY)).await;
    raw.expect_close(1009).await;
    let mut raw = Raw::connect(addr).await;
    raw.handshake("/echo").await;
    let mut header = vec![0x82, 0x80 | 127];
    header.extend_from_slice(&(1u64 << 30).to_be_bytes());
    header.extend_from_slice(&KEY);
    raw.send(&header).await;
    raw.expect_close(1009).await;
}

// ---------------------------------------------------------------- закрытие

#[tokio::test]
async fn client_initiated_close_handshake() {
    let (addr, log) = start(WsConfig::default()).await;
    let mut ws = connect(addr, "test", "/collect", None, &WsConfig::default()).await.unwrap();
    ws.send(Message::Text("last words".to_string())).await.unwrap();
    let started = Instant::now();
    within("закрытие", ws.close(1000, "done")).await.unwrap();
    assert!(started.elapsed() < Duration::from_secs(3), "сервер отвечает на Close сразу, ждать таймаута не нужно");
    wait_log(&log, "text:last words").await;
    wait_log(&log, "eof:Some(1000)").await;
}

#[tokio::test]
async fn server_initiated_close_handshake() {
    let (addr, _log) = start(WsConfig::default()).await;
    let mut ws = connect(addr, "test", "/closer", None, &WsConfig::default()).await.unwrap();
    assert_eq!(within("hello", ws.recv()).await.unwrap(), Some(Message::Text("hello".to_string())));
    assert_eq!(within("закрытие", ws.recv()).await.unwrap(), None, "после Close recv возвращает None");
    assert_eq!(ws.close_code(), Some(1000));
    match ws.send(Message::Text("late".to_string())).await {
        Err(WsError::Closed) => {}
        other => panic!("отправка после закрытия должна вернуть Closed, а вернула {other:?}"),
    }
}

#[tokio::test]
async fn peer_close_code_is_echoed() {
    let (addr, _log) = start(WsConfig::default()).await;
    let mut raw = Raw::connect(addr).await;
    raw.handshake("/echo").await;
    raw.send(&client_frame(0x88, &encode_close(1001, "going away"), KEY)).await;
    raw.expect_close(1001).await;
    raw.expect_eof().await;
}

#[tokio::test]
async fn invalid_close_codes_are_protocol_errors() {
    let (addr, _log) = start(WsConfig::default()).await;
    for body in [vec![0x03u8], vec![0x03, 0xec], vec![0x03, 0xe8, 0xff]] {
        let mut raw = Raw::connect(addr).await;
        raw.handshake("/echo").await;
        raw.send(&client_frame(0x88, &body, KEY)).await;
        let (b0, payload) = raw.frame().await.expect("ответный Close");
        assert_eq!(b0, 0x88);
        let code = u16::from_be_bytes([payload[0], payload[1]]);
        assert!(code == 1002 || code == 1007, "на плохое тело Close {body:?} ждали 1002 или 1007, получили {code}");
    }
}

#[tokio::test]
async fn silent_peer_cannot_block_the_close_forever() {
    let config = WsConfig { close_timeout: Duration::from_millis(300), ..WsConfig::default() };
    let (addr, log) = start(config).await;
    let mut raw = Raw::connect(addr).await;
    raw.handshake("/silent").await;
    raw.expect_close(1000).await;
    let line = wait_log(&log, "close_ms:").await;
    let ms: u128 = line.trim_start_matches("close_ms:").parse().unwrap();
    assert!((250..2500).contains(&ms), "close() ждал ответ {ms} мс при сроке 300 мс: срок должен соблюдаться, а не быть бесконечным или нулевым");
}

// ---------------------------------------------------------------- клиент

async fn fake_server(reply: impl FnOnce(String) -> Vec<u8> + Send + 'static) -> (SocketAddr, tokio::task::JoinHandle<TcpStream>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut buf = Vec::new();
        let mut chunk = [0u8; 2048];
        while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
            let n = stream.read(&mut chunk).await.unwrap();
            assert!(n > 0, "клиент закрыл соединение, не договорив запрос");
            buf.extend_from_slice(&chunk[..n]);
        }
        let request = String::from_utf8_lossy(&buf).into_owned();
        stream.write_all(&reply(request)).await.unwrap();
        stream.flush().await.unwrap();
        stream
    });
    (addr, task)
}

fn good_reply(request: &str) -> Vec<u8> {
    let key = header_of(request, "sec-websocket-key");
    assert_eq!(key.len(), 1, "клиент обязан прислать ровно один Sec-WebSocket-Key: {request:?}");
    assert_eq!(base64_decode(&key[0]).unwrap().len(), 16, "ключ клиента: 16 байт в base64");
    use sha1::{Digest, Sha1};
    let mut hasher = Sha1::new();
    hasher.update(key[0].as_bytes());
    hasher.update(b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11");
    let accept = base64_encode(&hasher.finalize());
    format!("HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n\r\n").into_bytes()
}

#[tokio::test]
async fn client_request_is_a_proper_upgrade() {
    let (addr, task) = fake_server(|request| {
        assert!(request.starts_with("GET /chat?room=1 HTTP/1.1\r\n"), "{request:?}");
        assert_eq!(header_of(&request, "host"), vec!["chat.example"]);
        assert_eq!(header_of(&request, "sec-websocket-version"), vec!["13"]);
        assert!(header_of(&request, "upgrade").iter().any(|v| v.eq_ignore_ascii_case("websocket")));
        assert!(header_of(&request, "connection").iter().any(|v| v.eq_ignore_ascii_case("upgrade")));
        assert_eq!(header_of(&request, "origin"), vec!["https://app.example"]);
        good_reply(&request)
    })
    .await;
    let ws = within("подключение", connect(addr, "chat.example", "/chat?room=1", Some("https://app.example"), &WsConfig::default())).await;
    assert!(ws.is_ok(), "клиент не принял правильный ответ: {:?}", ws.err());
    drop(task.await.unwrap());
}

#[tokio::test]
async fn client_masks_every_frame_with_a_fresh_key() {
    let (addr, task) = fake_server(|request| good_reply(&request)).await;
    let mut ws = within("подключение", connect(addr, "h", "/", None, &WsConfig::default())).await.unwrap();
    let mut server = task.await.unwrap();
    let mut keys = Vec::new();
    for i in 0..6u8 {
        let text = format!("message number {i}");
        ws.send(Message::Text(text.clone())).await.unwrap();
        let mut head = [0u8; 2];
        within("заголовок фрейма", server.read_exact(&mut head)).await.unwrap();
        assert_eq!(head[0], 0x81);
        assert_ne!(head[1] & 0x80, 0, "клиентский фрейм обязан быть замаскирован");
        let len = usize::from(head[1] & 0x7f);
        let mut rest = vec![0u8; 4 + len];
        within("тело фрейма", server.read_exact(&mut rest)).await.unwrap();
        let key = [rest[0], rest[1], rest[2], rest[3]];
        let plain: Vec<u8> = rest[4..].iter().enumerate().map(|(i, b)| b ^ key[i % 4]).collect();
        assert_eq!(plain, text.as_bytes());
        keys.push(key);
    }
    keys.sort_unstable();
    keys.dedup();
    assert!(keys.len() >= 5, "из 6 фреймов различных ключей только {}: ключ маски должен быть свежим для каждого фрейма", keys.len());
}

#[tokio::test]
async fn client_keeps_bytes_that_follow_the_101() {
    let (addr, task) = fake_server(|request| {
        let mut out = good_reply(&request);
        out.extend_from_slice(b"\x81\x05hello");
        out
    })
    .await;
    let mut ws = within("подключение", connect(addr, "h", "/", None, &WsConfig::default())).await.unwrap();
    assert_eq!(within("hello", ws.recv()).await.unwrap(), Some(Message::Text("hello".to_string())), "фрейм, пришедший вместе с ответом 101, не должен потеряться");
    drop(task.await.unwrap());
}

#[tokio::test]
async fn client_rejects_bad_answers() {
    type Reply = Box<dyn FnOnce(String) -> Vec<u8> + Send>;
    let answers: Vec<(&str, Reply)> = vec![
        ("200 вместо 101", Box::new(|_| b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n".to_vec())),
        (
            "чужой Accept",
            Box::new(|_| b"HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo=\r\n\r\n".to_vec()),
        ),
        (
            "нет Accept",
            Box::new(|_| b"HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\r\n".to_vec()),
        ),
        (
            "расширение, о котором не просили",
            Box::new(|request| {
                let mut out = good_reply(&request);
                let text = String::from_utf8(out.split_off(0)).unwrap().replace("\r\n\r\n", "\r\nSec-WebSocket-Extensions: permessage-deflate\r\n\r\n");
                text.into_bytes()
            }),
        ),
    ];
    for (what, reply) in answers {
        let (addr, task) = fake_server(reply).await;
        let result = within(what, connect(addr, "h", "/", None, &WsConfig::default())).await;
        assert!(result.is_err(), "клиент принял плохой ответ: {what}");
        drop(task.await.unwrap());
    }
}

// ---------------------------------------------------------------- вместе

#[tokio::test]
async fn many_clients_at_once() {
    let (addr, _log) = start(WsConfig::default()).await;
    let mut tasks = Vec::new();
    for c in 0..30 {
        tasks.push(tokio::spawn(async move {
            let mut ws = connect(addr, "t", "/echo", None, &WsConfig::default()).await.unwrap();
            for i in 0..5 {
                let text = format!("client {c} message {i}");
                ws.send(Message::Text(text.clone())).await.unwrap();
                assert_eq!(ws.recv().await.unwrap(), Some(Message::Text(text)));
            }
            ws.close(1000, "").await.unwrap();
        }));
    }
    within("30 клиентов", async {
        for t in tasks {
            t.await.unwrap();
        }
    })
    .await;
}

#[tokio::test]
async fn plain_http_keeps_working_next_to_websockets() {
    let (addr, _log) = start(WsConfig::default()).await;
    let _ws = connect(addr, "t", "/echo", None, &WsConfig::default()).await.unwrap();
    let mut raw = Raw::connect(addr).await;
    let head = raw.request("GET /health HTTP/1.1\r\nHost: t\r\n\r\n").await;
    assert_eq!(status_of(&head), 200, "обычные запросы продолжают работать, пока открыт сокет");
    let mut second = Raw::connect(addr).await;
    let head = second.request("GET /nothing HTTP/1.1\r\nHost: t\r\nConnection: close\r\n\r\n").await;
    assert_eq!(status_of(&head), 404);
}

// ---------------------------------------------------------------- кодек напрямую

struct Rng(u64);

impl Rng {
    fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next_u64() % n
    }
}

#[test]
fn codec_roundtrip_with_random_splits() {
    let mut rng = Rng(0x1234_5678_9abc_def1);
    let mut keys = MaskGen::with_seed(99);
    let sizes = [0usize, 1, 2, 124, 125, 126, 127, 1000, 65535, 65536, 70000];
    for case in 0..120 {
        let mut frames = Vec::new();
        for _ in 0..1 + rng.below(3) {
            let len = if rng.below(2) == 0 { sizes[rng.below(sizes.len() as u64) as usize] } else { rng.below(400) as usize };
            let opcode = [Opcode::Text, Opcode::Binary, Opcode::Continuation][rng.below(3) as usize];
            let payload: Vec<u8> = (0..len).map(|_| rng.next_u64() as u8).collect();
            frames.push(Frame { fin: rng.below(2) == 0, opcode, payload });
        }
        for role in [Role::Server, Role::Client] {
            let mut wire = BytesMut::new();
            for f in &frames {
                encode_frame(f, if role == Role::Server { Some(keys.next_key()) } else { None }, &mut wire);
            }
            let decoder = Decoder::new(role, 1 << 20);
            let mut buf = BytesMut::new();
            let mut got = Vec::new();
            let mut at = 0;
            while at < wire.len() {
                let n = (1 + rng.below(if wire.len() > 2000 { 4000 } else { 12 }) as usize).min(wire.len() - at);
                buf.extend_from_slice(&wire[at..at + n]);
                at += n;
                while let Some(frame) = decoder.decode(&mut buf).unwrap_or_else(|e| panic!("случай {case}, роль {role:?}: {e:?}")) {
                    got.push(frame);
                }
            }
            assert_eq!(got, frames, "случай {case}, роль {role:?}: кодек потерял или исказил фреймы");
            assert!(buf.is_empty(), "после чтения всего потока в буфере остался мусор");
        }
    }
}

#[test]
fn close_body_helpers() {
    assert_eq!(parse_close(&[]).unwrap(), None);
    assert_eq!(parse_close(&encode_close(1001, "bye")).unwrap(), Some((1001, "bye".to_string())));
    assert!(parse_close(&[0x03]).is_err());
    assert!(parse_close(&[0x03, 0xe9 + 3]).is_err(), "1004 нельзя передавать по сети");
    let body = encode_close(1000, &"я".repeat(100));
    assert!(body.len() <= 125 && std::str::from_utf8(&body[2..]).is_ok());
}
