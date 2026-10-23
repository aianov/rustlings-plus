use ntls3::serve;
use std::net::SocketAddr;
use std::time::{Duration, Instant};
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

const HANDSHAKE: Duration = Duration::from_millis(400);

async fn start() -> (SocketAddr, String) {
    let ca = Ca::new("тестовый CA");
    let leaf = ca.issue(&["localhost", "127.0.0.1"], Usage::Server);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(serve(listener, kit_server_config(&leaf.cert_pem, &leaf.key_pem), HANDSHAKE));
    (addr, ca.cert_pem())
}

async fn echo(addr: SocketAddr, ca_pem: &str, text: &[u8]) -> std::io::Result<Vec<u8>> {
    let tcp = TcpStream::connect(addr).await?;
    let mut tls = TlsConnector::from(kit_client_config(ca_pem)).connect(server_name("localhost"), tcp).await?;
    tls.write_all(text).await?;
    tls.shutdown().await?;
    let mut back = Vec::new();
    tls.read_to_end(&mut back).await?;
    Ok(back)
}

#[tokio::test]
async fn client_gets_its_bytes_back_over_tls() {
    let (addr, ca) = start().await;
    let back = within("эхо", echo(addr, &ca, b"hello, tls")).await.expect("эхо по TLS");
    assert_eq!(back, b"hello, tls");
}

#[tokio::test]
async fn large_message_survives() {
    let (addr, ca) = start().await;
    let big: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
    let back = within("большое эхо", echo(addr, &ca, &big)).await.expect("эхо");
    assert_eq!(back.len(), big.len());
    assert!(back == big, "эхо исказило данные");
}

#[tokio::test]
async fn silent_client_does_not_block_others_and_is_dropped() {
    let (addr, ca) = start().await;
    let mut silent = TcpStream::connect(addr).await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    let started = Instant::now();
    let back = within("эхо при молчащем соседе", echo(addr, &ca, b"ping")).await.expect("нормальный клиент должен обслуживаться, пока рядом висит молчащий");
    assert_eq!(back, b"ping");
    assert!(started.elapsed() < Duration::from_millis(300), "клиента задержало чужое рукопожатие ({:?}): рукопожатие должно идти в отдельной задаче", started.elapsed());
    let mut buf = [0u8; 16];
    let closed = tokio::time::timeout(Duration::from_secs(3), silent.read(&mut buf)).await.expect("молчащего клиента должны были отбросить по сроку рукопожатия");
    assert!(matches!(closed, Ok(0) | Err(_)), "соединение должно быть закрыто сервером");
}

#[tokio::test]
async fn non_tls_client_is_dropped_and_the_next_one_is_served() {
    let (addr, ca) = start().await;
    let mut plain = TcpStream::connect(addr).await.unwrap();
    plain.write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n").await.unwrap();
    let mut buf = [0u8; 256];
    let _ = tokio::time::timeout(Duration::from_secs(3), plain.read(&mut buf)).await.expect("не-TLS клиента должны были отбросить");
    let back = within("эхо", echo(addr, &ca, b"after")).await.expect("после чужого протокола сервер должен работать");
    assert_eq!(back, b"after");
}

#[tokio::test]
async fn untrusted_client_does_not_take_the_server_down() {
    let (addr, ca) = start().await;
    let stranger = Ca::new("чужой CA");
    let err = within("эхо", echo(addr, &stranger.cert_pem(), b"x")).await.expect_err("чужой CA не доверен клиентом");
    assert!(!err.to_string().is_empty());
    let back = within("эхо", echo(addr, &ca, b"still alive")).await.expect("сервер должен пережить неудачное рукопожатие");
    assert_eq!(back, b"still alive");
}

#[tokio::test]
async fn many_clients_in_parallel() {
    let (addr, ca) = start().await;
    let mut tasks = Vec::new();
    for i in 0..20u8 {
        let ca = ca.clone();
        tasks.push(tokio::spawn(async move { echo(addr, &ca, &[i; 100]).await.map(|b| (i, b)) }));
    }
    for task in tasks {
        let (i, back) = within("параллельное эхо", task).await.unwrap().expect("эхо");
        assert_eq!(back, vec![i; 100]);
    }
}
