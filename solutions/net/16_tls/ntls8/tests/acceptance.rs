use ntls8::{serve_plain, serve_tls};
use std::net::SocketAddr;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::TlsConnector;

#[path = "../src/pki.rs"]
mod pki;
use pki::{Ca, Usage};

use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName};
use rustls::{ClientConfig, RootCertStore, ServerConfig};
use std::sync::Arc;

#[allow(dead_code)]
fn certs_of(pem: &str) -> Vec<CertificateDer<'static>> {
    CertificateDer::pem_slice_iter(pem.as_bytes()).collect::<Result<_, _>>().expect("в тесте сертификаты корректны")
}

#[allow(dead_code)]
fn kit_server_config(cert_pem: &str, key_pem: &str) -> Arc<ServerConfig> {
    let key = PrivateKeyDer::from_pem_slice(key_pem.as_bytes()).expect("в тесте ключ корректен");
    Arc::new(ServerConfig::builder().with_no_client_auth().with_single_cert(certs_of(cert_pem), key).expect("в тесте пара подходит"))
}

#[allow(dead_code)]
fn kit_client_config(ca_pem: &str) -> Arc<ClientConfig> {
    let mut roots = RootCertStore::empty();
    for cert in certs_of(ca_pem) {
        roots.add(cert).expect("в тесте CA корректен");
    }
    Arc::new(ClientConfig::builder().with_root_certificates(roots).with_no_client_auth())
}

#[allow(dead_code)]
fn server_name(name: &str) -> ServerName<'static> {
    ServerName::try_from(name.to_string()).expect("в тесте имя корректно")
}

async fn within<T>(what: &str, f: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(std::time::Duration::from_secs(5), f)
        .await
        .unwrap_or_else(|_| panic!("{what}: не дождались ответа за 5 секунд"))
}

const GET: &[u8] = b"GET /hello HTTP/1.1\r\nHost: localhost\r\n\r\n";

async fn https_server(ca: &Ca) -> SocketAddr {
    let leaf = ca.issue(&["localhost"], Usage::Server);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(serve_tls(listener, kit_server_config(&leaf.cert_pem, &leaf.key_pem)));
    addr
}

async fn get_plain(addr: SocketAddr) -> String {
    let mut tcp = TcpStream::connect(addr).await.unwrap();
    tcp.write_all(GET).await.unwrap();
    let mut text = String::new();
    tcp.read_to_string(&mut text).await.unwrap();
    text
}

async fn get_tls(addr: SocketAddr, ca_pem: &str) -> std::io::Result<String> {
    let tcp = TcpStream::connect(addr).await?;
    let mut tls = TlsConnector::from(kit_client_config(ca_pem)).connect(server_name("localhost"), tcp).await?;
    tls.write_all(GET).await?;
    let mut text = String::new();
    tls.read_to_string(&mut text).await?;
    Ok(text)
}

#[tokio::test]
async fn https_answer_equals_http_answer() {
    let plain = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let plain_addr = plain.local_addr().unwrap();
    tokio::spawn(serve_plain(plain));
    let ca = Ca::new("тестовый CA");
    let secure_addr = https_server(&ca).await;
    let over_http = within("HTTP", get_plain(plain_addr)).await;
    let over_https = within("HTTPS", get_tls(secure_addr, &ca.cert_pem())).await.expect("ответ по HTTPS");
    assert!(over_http.starts_with("HTTP/1.1 200 OK"), "обработчик отвечает по HTTP: {over_http}");
    assert_eq!(over_https, over_http, "один и тот же обработчик должен давать один и тот же ответ поверх TLS");
}

#[tokio::test]
async fn client_reading_to_the_end_gets_a_clean_close() {
    let ca = Ca::new("тестовый CA");
    let addr = https_server(&ca).await;
    let text = within("HTTPS", get_tls(addr, &ca.cert_pem())).await.expect("чтение до конца потока: сервер должен закрыть TLS корректно (close_notify), а не просто бросить сокет");
    assert!(text.ends_with("привет из обработчика"));
}

#[tokio::test]
async fn bad_handshake_does_not_affect_the_next_client() {
    let ca = Ca::new("тестовый CA");
    let addr = https_server(&ca).await;
    let mut plain = TcpStream::connect(addr).await.unwrap();
    plain.write_all(GET).await.unwrap();
    let mut sink = Vec::new();
    let _ = tokio::time::timeout(std::time::Duration::from_secs(3), plain.read_to_end(&mut sink)).await;
    let text = within("HTTPS", get_tls(addr, &ca.cert_pem())).await.expect("после чужого протокола сервер должен жить");
    assert!(text.starts_with("HTTP/1.1 200 OK"));
}

#[tokio::test]
async fn parallel_https_clients() {
    let ca = Ca::new("тестовый CA");
    let addr = https_server(&ca).await;
    let pem = ca.cert_pem();
    let mut tasks = Vec::new();
    for _ in 0..16 {
        let pem = pem.clone();
        tasks.push(tokio::spawn(async move { get_tls(addr, &pem).await }));
    }
    for task in tasks {
        let text = within("HTTPS", task).await.unwrap().expect("параллельный клиент");
        assert!(text.starts_with("HTTP/1.1 200 OK"));
    }
}
