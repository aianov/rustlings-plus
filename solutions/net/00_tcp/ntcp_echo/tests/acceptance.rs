use ntcp_echo::{EchoError, echo_roundtrip, serve_one};
use std::future::Future;
use std::io;
use std::net::SocketAddr;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::timeout;

const IDLE: Duration = Duration::from_secs(5);

async fn within<T>(what: &str, fut: impl Future<Output = T>) -> T {
    match timeout(Duration::from_secs(9), fut).await {
        Ok(v) => v,
        Err(_) => panic!("{what}: не уложились в 9 секунд, похоже на взаимное ожидание или бесконечный цикл"),
    }
}

async fn listener() -> (TcpListener, SocketAddr) {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = l.local_addr().unwrap();
    (l, addr)
}

fn pattern(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i % 251) as u8).collect()
}

async fn roundtrip(len: usize) {
    let (l, addr) = listener().await;
    let server = tokio::spawn(async move { serve_one(&l, IDLE).await });
    let data = pattern(len);
    let echoed = within("клиент", echo_roundtrip(addr, data.clone()))
        .await
        .expect("эхо-обмен должен завершиться успешно");
    assert_eq!(echoed.len(), data.len(), "эхо пришло длиной {} вместо {}", echoed.len(), data.len());
    assert!(echoed == data, "длина совпала, но содержимое эха отличается от отправленного");
    let served = within("сервер", server).await.unwrap().expect("сервер должен закончить без ошибки");
    assert_eq!(served, len as u64, "serve_one должен вернуть число эхо-байтов");
}

#[tokio::test]
async fn one_megabyte_comes_back() {
    roundtrip(1024 * 1024).await;
}

#[tokio::test]
async fn eight_megabytes_do_not_deadlock() {
    roundtrip(8 * 1024 * 1024).await;
}

#[tokio::test]
async fn empty_payload_is_fine() {
    roundtrip(0).await;
}

#[tokio::test]
async fn refused_is_named_refused() {
    let (l, addr) = listener().await;
    drop(l);
    let res = within("клиент", echo_roundtrip(addr, vec![1, 2, 3])).await;
    assert!(matches!(res, Err(EchoError::Refused)), "порт никто не слушает, ждали Refused, а вышло {res:?}");
}

#[tokio::test]
async fn silent_client_is_dropped_by_idle_timeout() {
    let (l, addr) = listener().await;
    let server = tokio::spawn(async move { serve_one(&l, Duration::from_millis(200)).await });
    let _silent = TcpStream::connect(addr).await.unwrap();
    let started = Instant::now();
    let res = within("сервер", server).await.unwrap();
    assert!(matches!(res, Err(EchoError::TimedOut)), "клиент молчит дольше лимита простоя, ждали TimedOut, а вышло {res:?}");
    assert!(started.elapsed() < Duration::from_secs(4), "таймаут простоя сработал слишком поздно");
}

#[tokio::test]
async fn client_that_resets_is_named_reset() {
    let (l, addr) = listener().await;
    let server = tokio::spawn(async move { serve_one(&l, IDLE).await });
    let mut raw = TcpStream::connect(addr).await.unwrap();
    raw.write_all(b"hello").await.unwrap();
    within("эхо", raw.readable()).await.unwrap();
    drop(raw);
    let res = within("сервер", server).await.unwrap();
    assert!(matches!(res, Err(EchoError::Reset)), "клиент закрылся с непрочитанным эхом (reset), ждали Reset, а вышло {res:?}");
}

#[tokio::test]
async fn short_echo_is_closed() {
    let (l, addr) = listener().await;
    tokio::spawn(async move {
        let (mut s, _) = l.accept().await.unwrap();
        let mut request = [0u8; 100];
        s.read_exact(&mut request).await.unwrap();
        s.write_all(&request[..40]).await.unwrap();
    });
    let res = within("клиент", echo_roundtrip(addr, vec![9u8; 100])).await;
    assert!(
        matches!(res, Err(EchoError::Closed)),
        "сервер вернул 40 байт из 100 и закрылся: эхо обрезано, ждали Closed, а вышло {res:?}"
    );
}

#[test]
fn errors_speak_human() {
    let all = [
        EchoError::Refused,
        EchoError::Reset,
        EchoError::Closed,
        EchoError::TimedOut,
        EchoError::Io(io::Error::other("диск сгорел")),
    ];
    let texts: Vec<String> = all.iter().map(|e| e.to_string()).collect();
    for (i, a) in texts.iter().enumerate() {
        assert!(!a.trim().is_empty(), "у варианта {i} пустое сообщение");
        for b in &texts[i + 1..] {
            assert_ne!(a, b, "разные ошибки показывают одно сообщение: «{a}»");
        }
    }
    assert!(texts[4].contains("диск сгорел"), "Io должен показывать текст исходной ошибки");
}

#[test]
fn io_errors_are_classified() {
    use io::ErrorKind::*;
    type Check = fn(&EchoError) -> bool;
    let cases: [(io::ErrorKind, Check); 4] = [
        (ConnectionRefused, |e| matches!(e, EchoError::Refused)),
        (ConnectionReset, |e| matches!(e, EchoError::Reset)),
        (UnexpectedEof, |e| matches!(e, EchoError::Closed)),
        (TimedOut, |e| matches!(e, EchoError::TimedOut)),
    ];
    for (kind, check) in cases {
        let e = EchoError::from(io::Error::from(kind));
        assert!(check(&e), "{kind:?} разложен не по тому варианту: {e:?}");
    }
    assert!(matches!(EchoError::from(io::Error::from(NotFound)), EchoError::Io(_)), "остальные ошибки идут в Io");
}
