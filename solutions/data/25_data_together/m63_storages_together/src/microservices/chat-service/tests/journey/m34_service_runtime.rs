use std::time::Duration;

use chat_service::{Config, serve};
use service_runtime::{CancellationToken, Source};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{Instant, sleep};

#[test]
fn config_reads_chat_settings_and_names_every_mistake() {
    let config = Config::from_source(&Source::from_pairs([("USERS_GRPC_ADDR", "10.0.0.2:50052")]))
        .expect("всё верно");
    assert_eq!(config.addr, "127.0.0.1:50054");
    assert_eq!(config.admin, "127.0.0.1:9884");
    assert_eq!(config.users, "10.0.0.2:50052");
    assert!(
        config.stores.is_none(),
        "с m63 по переменным окружения память не подставляется"
    );
    let error = Config::from_source(&Source::from_pairs([
        ("CHAT_GRPC_ADDR", "127.0.0.1:x"),
        ("STARTUP_WAIT_MS", "-1"),
        ("STARTUP_RETRY_MS", "1.5"),
    ]))
    .expect_err("три ошибки");
    assert_eq!(
        error.keys(),
        ["CHAT_GRPC_ADDR", "STARTUP_RETRY_MS", "STARTUP_WAIT_MS"],
        "{error}"
    );
}

async fn ready(addr: &str) -> (u16, String) {
    let Ok(mut stream) = TcpStream::connect(addr).await else {
        return (0, String::new());
    };
    let ask = "GET /ready HTTP/1.1\r\nHost: admin\r\nConnection: close\r\n\r\n";
    let _ = stream.write_all(ask.as_bytes()).await;
    let mut raw = String::new();
    let _ = stream.read_to_string(&mut raw).await;
    let status = raw
        .split(' ')
        .nth(1)
        .and_then(|code| code.parse().ok())
        .unwrap_or(0);
    (status, raw)
}

#[tokio::test]
async fn chat_is_not_ready_without_user_service() {
    let users = TcpListener::bind("127.0.0.1:0").await.expect("порт");
    let users_addr = users.local_addr().expect("адрес").to_string();
    drop(users);
    let chat = TcpListener::bind("127.0.0.1:0").await.expect("порт");
    let admin = TcpListener::bind("127.0.0.1:0").await.expect("порт");
    let admin_addr = admin.local_addr().expect("адрес").to_string();
    let stop = CancellationToken::new();
    let config = Config {
        users: users_addr,
        ..Config::default()
    };
    let task = tokio::spawn(serve(chat, admin, config, stop.clone()));
    let started = Instant::now();
    loop {
        let (status, body) = ready(&admin_addr).await;
        if status == 503 && body.contains("user-service") {
            break;
        }
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "{status} {body}"
        );
        sleep(Duration::from_millis(20)).await;
    }
    stop.cancel();
    task.await
        .expect("chat-service")
        .expect("останавливается чисто");
}
