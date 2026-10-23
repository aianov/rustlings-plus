use ntls10::{client_config, server_config};
use std::net::SocketAddr;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::{TlsAcceptor, TlsConnector};
use rustls::pki_types::ServerName;

#[path = "../src/pki.rs"]
mod pki;
use pki::{Ca, Leaf, Usage};

async fn within<T>(what: &str, f: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(std::time::Duration::from_secs(5), f)
        .await
        .unwrap_or_else(|_| panic!("{what}: не дождались ответа за 5 секунд"))
}

struct World {
    server_ca: Ca,
    client_ca: Ca,
    addr: SocketAddr,
}

async fn world() -> World {
    let server_ca = Ca::new("серверный CA");
    let client_ca = Ca::new("клиентский CA");
    let leaf = server_ca.issue(&["localhost"], Usage::Server);
    let config = server_config(&leaf.cert_pem, &leaf.key_pem, &client_ca.cert_pem()).expect("конфигурация сервера");
    let acceptor = TlsAcceptor::from(config);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let Ok((tcp, _)) = listener.accept().await else { continue };
            let acceptor = acceptor.clone();
            tokio::spawn(async move {
                let Ok(mut tls) = acceptor.accept(tcp).await else { return };
                let mut data = Vec::new();
                if tls.read_to_end(&mut data).await.is_ok() {
                    let _ = tls.write_all(&data).await;
                    let _ = tls.shutdown().await;
                }
            });
        }
    });
    World { server_ca, client_ca, addr }
}

async fn roundtrip(w: &World, server_trust: &Ca, identity: Option<&Leaf>) -> Result<(), String> {
    let pair = identity.map(|l| (l.cert_pem.as_str(), l.key_pem.as_str()));
    let config = client_config(&server_trust.cert_pem(), pair)?;
    let exchange = async {
        let tcp = TcpStream::connect(w.addr).await.map_err(|e| e.to_string())?;
        let mut tls = TlsConnector::from(config).connect(ServerName::try_from("localhost").unwrap(), tcp).await.map_err(|e| e.to_string())?;
        tls.write_all(b"ping").await.map_err(|e| e.to_string())?;
        tls.shutdown().await.map_err(|e| e.to_string())?;
        let mut reply = Vec::new();
        tls.read_to_end(&mut reply).await.map_err(|e| e.to_string())?;
        if reply == b"ping" { Ok(()) } else { Err(format!("эхо не пришло: {reply:?}")) }
    };
    within("обмен", exchange).await
}

#[tokio::test]
async fn client_with_a_certificate_from_the_right_ca_is_served() {
    let w = world().await;
    let me = w.client_ca.issue(&["gateway.social.test"], Usage::Client);
    roundtrip(&w, &w.server_ca, Some(&me)).await.expect("клиент с сертификатом нужного CA");
}

#[tokio::test]
async fn client_without_a_certificate_is_refused() {
    let w = world().await;
    let err = roundtrip(&w, &w.server_ca, None).await.expect_err("сервер обязан требовать клиентский сертификат");
    assert!(!err.is_empty());
}

#[tokio::test]
async fn certificate_from_another_ca_is_refused() {
    let w = world().await;
    let stranger = Ca::new("чужой CA");
    let fake = stranger.issue(&["gateway.social.test"], Usage::Client);
    roundtrip(&w, &w.server_ca, Some(&fake)).await.expect_err("сертификат клиента подписан не тем CA");
}

#[tokio::test]
async fn expired_client_certificate_is_refused() {
    let w = world().await;
    let old = w.client_ca.issue_valid(&["gateway.social.test"], Usage::Client, (2000, 1, 1), (2001, 1, 1));
    roundtrip(&w, &w.server_ca, Some(&old)).await.expect_err("клиентский сертификат просрочен");
}

#[tokio::test]
async fn server_certificate_cannot_pose_as_a_client_one() {
    let w = world().await;
    let wrong_usage = w.client_ca.issue(&["gateway.social.test"], Usage::Server);
    roundtrip(&w, &w.server_ca, Some(&wrong_usage)).await.expect_err("сертификат помечен «только для серверов»");
}

#[tokio::test]
async fn client_that_does_not_trust_the_server_gives_up_itself() {
    let w = world().await;
    let me = w.client_ca.issue(&["gateway.social.test"], Usage::Client);
    let err = roundtrip(&w, &w.client_ca, Some(&me)).await.expect_err("клиент доверяет не тому CA");
    assert!(err.contains("UnknownIssuer"), "ждали UnknownIssuer, получили {err}");
}

#[test]
fn broken_inputs_are_errors() {
    let ca = Ca::new("CA");
    let leaf = ca.issue(&["localhost"], Usage::Server);
    assert!(server_config(&leaf.cert_pem, &leaf.key_pem, "").is_err(), "пустой список доверия клиентам");
    assert!(server_config(&leaf.cert_pem, &leaf.key_pem, "просто текст").is_err());
    let other = ca.issue(&["localhost"], Usage::Server);
    assert!(server_config(&leaf.cert_pem, &other.key_pem, &ca.cert_pem()).is_err(), "ключ от другого сертификата");
    assert!(client_config("", None).is_err());
}
