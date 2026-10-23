use ntls_tonic::pb::users_client::UsersClient;
use ntls_tonic::pb::GetUserRequest;
use ntls_tonic::{client_channel, serve_tls};
use std::net::SocketAddr;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tonic::transport::Endpoint;
use tonic::Code;

#[path = "../src/pki.rs"]
mod pki;
use pki::{Ca, Usage};

async fn within<T>(what: &str, f: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(std::time::Duration::from_secs(5), f)
        .await
        .unwrap_or_else(|_| panic!("{what}: не дождались ответа за 5 секунд"))
}

async fn server(ca: &Ca, names: &[&str]) -> SocketAddr {
    let leaf = ca.issue(names, Usage::Server);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = serve_tls(listener, &leaf.cert_pem, &leaf.key_pem).await;
    });
    addr
}

async fn get(addr: SocketAddr, ca_pem: &str, domain: &str) -> Result<String, tonic::Status> {
    let channel = client_channel(addr, ca_pem, domain).expect("канал");
    let mut client = UsersClient::new(channel);
    Ok(within("вызов", client.get_user(GetUserRequest { id: 7 })).await?.into_inner().handle)
}

#[tokio::test]
async fn call_over_tls_works() {
    let ca = Ca::new("тестовый CA");
    let addr = server(&ca, &["localhost", "127.0.0.1"]).await;
    assert_eq!(get(addr, &ca.cert_pem(), "localhost").await.expect("вызов по TLS"), "user7");
}

#[tokio::test]
async fn name_is_checked_against_the_domain_not_the_address() {
    let ca = Ca::new("тестовый CA");
    let addr = server(&ca, &["users.social.test"]).await;
    assert_eq!(get(addr, &ca.cert_pem(), "users.social.test").await.expect("подключение по IP, проверка по имени из сертификата"), "user7");
    let err = get(addr, &ca.cert_pem(), "posts.social.test").await.expect_err("сертификат выдан на другое имя");
    assert_eq!(err.code(), Code::Unavailable);
}

#[tokio::test]
async fn address_in_the_certificate_does_not_excuse_a_wrong_domain() {
    let ca = Ca::new("тестовый CA");
    let addr = server(&ca, &["localhost", "127.0.0.1"]).await;
    let err = get(addr, &ca.cert_pem(), "other.test").await.expect_err("проверка идёт по заданному имени, а не по адресу подключения");
    assert_eq!(err.code(), Code::Unavailable);
}

#[tokio::test]
async fn foreign_ca_is_not_trusted() {
    let ca = Ca::new("настоящий CA");
    let evil = Ca::new("чужой CA");
    let addr = server(&evil, &["localhost"]).await;
    let err = get(addr, &ca.cert_pem(), "localhost").await.expect_err("сервер представился сертификатом чужого CA");
    assert_eq!(err.code(), Code::Unavailable);
}

#[tokio::test]
async fn plain_text_client_gets_nothing() {
    let ca = Ca::new("тестовый CA");
    let addr = server(&ca, &["localhost"]).await;
    let channel = Endpoint::from_shared(format!("http://{addr}")).unwrap().connect_timeout(Duration::from_millis(500)).timeout(Duration::from_secs(2)).connect_lazy();
    let mut client = UsersClient::new(channel);
    let result = within("вызов", client.get_user(GetUserRequest { id: 1 })).await;
    assert!(result.is_err(), "сервер под TLS не должен отвечать открытому клиенту");
    let mut raw = TcpStream::connect(addr).await.unwrap();
    raw.write_all(b"GET / HTTP/1.1\r\nHost: x\r\n\r\n").await.unwrap();
    let mut buf = Vec::new();
    let _ = tokio::time::timeout(Duration::from_secs(2), raw.read_to_end(&mut buf)).await;
    assert!(!buf.starts_with(b"HTTP/"), "сервер ответил открытым HTTP: TLS не включён");
}
