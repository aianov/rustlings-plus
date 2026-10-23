use std::net::{IpAddr, SocketAddr};

use http_client::{AddrGuard, Client, Config, Error, public_ip};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

async fn serve(reply: String) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("порт");
    let addr = listener.local_addr().expect("адрес");
    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            let reply = reply.clone();
            tokio::spawn(async move {
                let mut buf = vec![0u8; 4096];
                let _ = socket.read(&mut buf).await;
                let _ = socket.write_all(reply.as_bytes()).await;
            });
        }
    });
    addr
}

fn page(body: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

fn guarded(guard: AddrGuard) -> Client {
    Client::new(Config {
        guard: Some(guard),
        max_idle_per_host: 0,
        ..Config::default()
    })
}

#[test]
fn only_public_addresses_pass() {
    for open in ["93.184.216.34", "8.8.8.8", "2606:4700:4700::1111"] {
        let ip: IpAddr = open.parse().expect("адрес");
        assert!(public_ip(ip), "{open} публичный");
    }
    for closed in [
        "127.0.0.1",
        "10.1.2.3",
        "172.16.0.1",
        "192.168.1.1",
        "169.254.169.254",
        "0.0.0.0",
        "100.64.0.1",
        "224.0.0.1",
        "255.255.255.255",
        "::1",
        "fe80::1",
        "fd00::1",
        "::ffff:127.0.0.1",
        "::ffff:10.0.0.1",
    ] {
        let ip: IpAddr = closed.parse().expect("адрес");
        assert!(!public_ip(ip), "{closed} внутренний");
    }
}

#[tokio::test]
async fn loopback_is_refused_before_connecting() {
    let addr = serve(page("<title>тайна</title>")).await;
    let client = guarded(AddrGuard::public_only());
    let refused = client.get(&format!("http://{addr}/")).await;
    assert!(matches!(refused, Err(Error::Forbidden(..))), "{refused:?}");
    let localhost = client
        .get(&format!("http://localhost:{}/", addr.port()))
        .await;
    assert!(
        matches!(localhost, Err(Error::Forbidden(..))),
        "имя тоже проверяется по адресу после резолва: {localhost:?}"
    );
}

#[tokio::test]
async fn a_redirect_is_checked_again() {
    let inner = serve(page("<title>внутреннее</title>")).await;
    let outer = serve(format!(
        "HTTP/1.1 302 Found\r\nLocation: http://{inner}/\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    ))
    .await;
    let allowed = outer.port();
    let client = guarded(AddrGuard::new(move |addr| addr.port() == allowed));
    let followed = client.get(&format!("http://{outer}/")).await;
    assert!(
        matches!(followed, Err(Error::Forbidden(..))),
        "редирект на закрытый адрес отвергнут: {followed:?}"
    );
    let open = guarded(AddrGuard::new(|_| true));
    let page = open
        .get(&format!("http://{outer}/"))
        .await
        .expect("без сторожа доходит");
    assert!(page.text().contains("внутреннее"));
}
