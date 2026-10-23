use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use http_client::{Client, Config, Error, Request};
use service_runtime::CancellationToken;
use tls_edge::{client_config, server_config, terminate};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

fn certs(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("m37-client")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    certgen::issue_all(&dir, false).expect("сертификаты выпускаются");
    dir
}

async fn answering(answer: &'static str) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("порт");
    let addr = listener.local_addr().expect("адрес");
    tokio::spawn(async move {
        while let Ok((mut tcp, _)) = listener.accept().await {
            tokio::spawn(async move {
                let mut raw = Vec::new();
                let mut chunk = [0u8; 1024];
                while !raw.windows(4).any(|w| w == b"\r\n\r\n") {
                    match tcp.read(&mut chunk).await {
                        Ok(0) | Err(..) => return,
                        Ok(n) => raw.extend_from_slice(&chunk[..n]),
                    }
                }
                let _ = tcp.write_all(answer.as_bytes()).await;
            });
        }
    });
    addr
}

async fn secured(dir: &std::path::Path) -> (SocketAddr, CancellationToken) {
    let read = |file: &str| fs::read(dir.join(file)).expect("файл сертификата");
    let inner = answering("HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello").await;
    let public = TcpListener::bind("127.0.0.1:0").await.expect("порт");
    let addr = public.local_addr().expect("адрес");
    let config =
        server_config(&read("auth-service.pem"), &read("auth-service.key")).expect("конфиг");
    let stop = CancellationToken::new();
    tokio::spawn(terminate(public, inner, config, stop.clone()));
    (addr, stop)
}

#[tokio::test]
async fn https_checks_the_name_from_config() {
    let dir = certs("https");
    let (addr, stop) = secured(&dir).await;
    let ca = fs::read(dir.join("ca.pem")).expect("ca.pem");
    let with_name = |name: &str| {
        Client::new(Config {
            tls: Some(client_config(&ca).expect("клиент")),
            tls_name: Some(name.to_string()),
            ..Config::default()
        })
    };
    let url = format!("https://{addr}/hello");
    let answer = with_name("auth-service")
        .get(&url)
        .await
        .expect("https с нашим CA и именем auth-service");
    assert_eq!((answer.status, answer.text().as_str()), (200, "hello"));
    assert!(answer.url.secure, "в ответе видно, что ходили по TLS");
    assert!(
        matches!(
            with_name("user-service").get(&url).await,
            Err(Error::Tls(..))
        ),
        "сертификат auth-service не подходит под имя user-service: адрес это IP, имя задаёт tls_name"
    );
    let plain = Client::new(Config::default());
    assert!(
        matches!(plain.get(&url).await, Err(Error::Url(..))),
        "https без настроек TLS не уходит открытым текстом"
    );
    stop.cancel();
}

#[tokio::test]
async fn relay_hands_back_redirects_untouched() {
    let addr = answering(
        "HTTP/1.1 302 Found\r\nLocation: /elsewhere\r\nSet-Cookie: a=1\r\nSet-Cookie: b=2\r\nContent-Length: 0\r\n\r\n",
    )
    .await;
    let client = Client::new(Config {
        total_timeout: Duration::from_secs(2),
        ..Config::default()
    });
    let relayed = client
        .relay(Request::get(&format!("http://{addr}/auth/login")))
        .await
        .expect("один обмен");
    assert_eq!(
        (
            relayed.status,
            relayed.header("location"),
            relayed.redirects
        ),
        (302, Some("/elsewhere"), 0),
        "прокси отдаёт редирект клиенту как есть, а не ходит по нему сам"
    );
    let cookies: Vec<&str> = relayed
        .headers
        .iter()
        .filter(|(name, _)| name.eq_ignore_ascii_case("set-cookie"))
        .map(|(_, value)| value.as_str())
        .collect();
    assert_eq!(
        cookies,
        ["a=1", "b=2"],
        "повторные заголовки не склеиваются"
    );
    let followed = client
        .send(Request::get(&format!("http://{addr}/auth/login")))
        .await;
    assert!(
        matches!(followed, Err(Error::TooManyRedirects)),
        "обычный send по редиректам ходит, и здесь упирается в предел"
    );
}
