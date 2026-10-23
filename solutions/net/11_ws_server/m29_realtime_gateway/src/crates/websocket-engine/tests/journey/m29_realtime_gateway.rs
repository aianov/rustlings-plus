use std::time::Duration;

use tokio::io::{DuplexStream, duplex};
use tokio::time::{sleep, timeout};
use websocket_engine::{Message, Role, WsConfig, WsStream};

fn pair() -> (WsStream<DuplexStream>, WsStream<DuplexStream>) {
    let (near, far) = duplex(1 << 16);
    let config = WsConfig::default();
    (
        WsStream::new(near, Vec::new(), Role::Server, &config),
        WsStream::new(far, Vec::new(), Role::Client, &config),
    )
}

#[tokio::test(start_paused = true)]
async fn idle_for_counts_from_the_last_bytes_heard() {
    let (mut server, mut client) = pair();
    sleep(Duration::from_millis(300)).await;
    assert!(
        server.idle_for() >= Duration::from_millis(300),
        "от клиента ничего не было 300 мс: {:?}",
        server.idle_for()
    );
    client
        .send(Message::Text(String::from("привет")))
        .await
        .expect("сообщение уходит");
    let got = server.recv().await.expect("сообщение читается");
    assert_eq!(got, Some(Message::Text(String::from("привет"))));
    assert!(
        server.idle_for() < Duration::from_millis(10),
        "сообщение это весть от клиента: {:?}",
        server.idle_for()
    );
    sleep(Duration::from_millis(200)).await;
    client.ping(b"").await.expect("ping уходит");
    let quiet = timeout(Duration::from_millis(50), server.recv()).await;
    assert!(quiet.is_err(), "ping не попадает к приложению");
    assert!(
        server.idle_for() <= Duration::from_millis(50),
        "ping тоже весть: часы сброшены, {:?}",
        server.idle_for()
    );
    sleep(Duration::from_millis(200)).await;
    server.ping(b"").await.expect("ping сервера уходит");
    let quiet = timeout(Duration::from_millis(50), client.recv()).await;
    assert!(quiet.is_err(), "клиент сам ответил pong");
    let quiet = timeout(Duration::from_millis(50), server.recv()).await;
    assert!(quiet.is_err(), "pong не попадает к приложению");
    assert!(
        server.idle_for() <= Duration::from_millis(100),
        "pong на ping сервера продлевает жизнь соединения: {:?}",
        server.idle_for()
    );
}
