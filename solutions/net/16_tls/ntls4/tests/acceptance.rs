use ntls4::{request, ClientError};
use std::net::SocketAddr;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;

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

const TIMEOUT: Duration = Duration::from_millis(600);

async fn upper_server(names: &[&str], ca: &Ca) -> SocketAddr {
    let leaf = ca.issue(names, Usage::Server);
    let acceptor = TlsAcceptor::from(kit_server_config(&leaf.cert_pem, &leaf.key_pem));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let Ok((tcp, _)) = listener.accept().await else { continue };
            let acceptor = acceptor.clone();
            tokio::spawn(async move {
                let Ok(mut tls) = acceptor.accept(tcp).await else { return };
                let mut data = Vec::new();
                if tls.read_to_end(&mut data).await.is_err() {
                    return;
                }
                let _ = tls.write_all(&data.to_ascii_uppercase()).await;
                let _ = tls.shutdown().await;
            });
        }
    });
    addr
}

#[tokio::test]
async fn request_by_dns_name_and_by_ip() {
    let ca = Ca::new("тестовый CA");
    let addr = upper_server(&["localhost", "127.0.0.1"], &ca).await;
    let reply = within("запрос", request(addr, "localhost", &ca.cert_pem(), b"hello", TIMEOUT)).await.expect("по имени localhost");
    assert_eq!(reply, b"HELLO");
    let reply = within("запрос", request(addr, "127.0.0.1", &ca.cert_pem(), b"abc", TIMEOUT)).await.expect("по IP");
    assert_eq!(reply, b"ABC");
}

#[tokio::test]
async fn connect_address_and_checked_name_are_independent() {
    let ca = Ca::new("тестовый CA");
    let addr = upper_server(&["db.internal.test"], &ca).await;
    let reply = within("запрос", request(addr, "db.internal.test", &ca.cert_pem(), b"x", TIMEOUT)).await.expect("проверка по имени из сертификата, а подключение по адресу 127.0.0.1");
    assert_eq!(reply, b"X");
    let err = within("запрос", request(addr, "localhost", &ca.cert_pem(), b"x", TIMEOUT)).await.expect_err("сертификат выдан не для localhost");
    assert!(matches!(err, ClientError::Io(_)), "ждали Io, получили {err:?}");
}

#[tokio::test]
async fn foreign_ca_is_rejected() {
    let ca = Ca::new("настоящий CA");
    let evil = Ca::new("чужой CA");
    let addr = upper_server(&["localhost"], &evil).await;
    let err = within("запрос", request(addr, "localhost", &ca.cert_pem(), b"x", TIMEOUT)).await.expect_err("сервер представился сертификатом чужого CA");
    assert!(matches!(err, ClientError::Io(ref m) if m.contains("UnknownIssuer")), "ждали Io с UnknownIssuer, получили {err:?}");
}

#[tokio::test]
async fn bad_input_is_an_error_not_a_panic() {
    let ca = Ca::new("тестовый CA");
    let addr = upper_server(&["localhost"], &ca).await;
    for bad in ["", "no spaces allowed", "a..b", "-bad.test"] {
        let err = request(addr, bad, &ca.cert_pem(), b"x", TIMEOUT).await.expect_err("плохое имя");
        assert!(matches!(err, ClientError::InvalidName(_)), "имя {bad:?}: ждали InvalidName, получили {err:?}");
    }
    for bad in ["", "просто текст", "-----BEGIN CERTIFICATE-----\nбитый\n-----END CERTIFICATE-----\n"] {
        let err = request(addr, "localhost", bad, b"x", TIMEOUT).await.expect_err("плохой CA");
        assert!(matches!(err, ClientError::BadCa(_)), "CA {bad:?}: ждали BadCa, получили {err:?}");
    }
}

#[tokio::test]
async fn plain_tcp_server_is_an_error() {
    let ca = Ca::new("тестовый CA");
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let Ok((mut tcp, _)) = listener.accept().await else { continue };
            tokio::spawn(async move {
                let _ = tcp.write_all(b"HTTP/1.1 200 OK\r\n\r\nnot tls\r\n").await;
            });
        }
    });
    let err = within("запрос", request(addr, "localhost", &ca.cert_pem(), b"x", TIMEOUT)).await.expect_err("сервер не говорит по TLS");
    assert!(matches!(err, ClientError::Io(_)), "ждали Io, получили {err:?}");
}

#[tokio::test]
async fn silent_server_hits_the_total_timeout() {
    let ca = Ca::new("тестовый CA");
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let mut held = Vec::new();
        loop {
            let Ok((tcp, _)) = listener.accept().await else { continue };
            held.push(tcp);
        }
    });
    let started = Instant::now();
    let err = within("запрос", request(addr, "localhost", &ca.cert_pem(), b"x", Duration::from_millis(300))).await.expect_err("сервер молчит");
    assert!(matches!(err, ClientError::Timeout), "ждали Timeout, получили {err:?}");
    assert!(started.elapsed() >= Duration::from_millis(250) && started.elapsed() < Duration::from_millis(900), "срок 300 мс, а ждали {:?}", started.elapsed());
}
