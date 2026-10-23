use ntls6::{classify, describe, try_connect, VerifyFailure};
use std::net::SocketAddr;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;

#[path = "../src/pki.rs"]
mod pki;
use pki::{Ca, Leaf, Usage};

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

async fn tls_server(leaf: &Leaf) -> SocketAddr {
    let acceptor = TlsAcceptor::from(kit_server_config(&leaf.cert_pem, &leaf.key_pem));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let Ok((tcp, _)) = listener.accept().await else { continue };
            let acceptor = acceptor.clone();
            tokio::spawn(async move {
                if let Ok(mut tls) = acceptor.accept(tcp).await {
                    let _ = tls.shutdown().await;
                }
            });
        }
    });
    addr
}

async fn failure(leaf: &Leaf, trusted: &Ca, name: &str) -> VerifyFailure {
    let addr = tls_server(leaf).await;
    let err = within("рукопожатие", try_connect(addr, name, &trusted.cert_pem())).await.expect_err("рукопожатие должно было не удаться");
    classify(&err)
}

#[tokio::test]
async fn working_handshake_is_not_a_failure() {
    let ca = Ca::new("мой CA");
    let leaf = ca.issue(&["localhost"], Usage::Server);
    let addr = tls_server(&leaf).await;
    within("рукопожатие", try_connect(addr, "localhost", &ca.cert_pem())).await.expect("верный сертификат");
}

#[tokio::test]
async fn certificate_from_another_ca() {
    let mine = Ca::new("мой CA");
    let evil = Ca::new("чужой CA");
    let leaf = evil.issue(&["localhost"], Usage::Server);
    assert_eq!(failure(&leaf, &mine, "localhost").await, VerifyFailure::UnknownIssuer);
}

#[tokio::test]
async fn certificate_for_another_name() {
    let ca = Ca::new("мой CA");
    let leaf = ca.issue(&["other.example.test"], Usage::Server);
    assert_eq!(failure(&leaf, &ca, "localhost").await, VerifyFailure::NameMismatch);
}

#[tokio::test]
async fn expired_certificate() {
    let ca = Ca::new("мой CA");
    let leaf = ca.issue_valid(&["localhost"], Usage::Server, (2000, 1, 1), (2001, 1, 1));
    assert_eq!(failure(&leaf, &ca, "localhost").await, VerifyFailure::Expired);
}

#[tokio::test]
async fn certificate_from_the_future() {
    let ca = Ca::new("мой CA");
    let leaf = ca.issue_valid(&["localhost"], Usage::Server, (2900, 1, 1), (2901, 1, 1));
    assert_eq!(failure(&leaf, &ca, "localhost").await, VerifyFailure::NotYetValid);
}

#[tokio::test]
async fn failures_that_are_not_about_certificates_are_other() {
    let ca = Ca::new("мой CA");
    let closer = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = closer.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let Ok((tcp, _)) = closer.accept().await else { continue };
            drop(tcp);
        }
    });
    let err = within("рукопожатие", try_connect(addr, "localhost", &ca.cert_pem())).await.expect_err("сервер закрыл соединение");
    assert!(matches!(classify(&err), VerifyFailure::Other(_)), "обрыв соединения это не про сертификаты: {:?}", classify(&err));
    let plain = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = plain.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let Ok((mut tcp, _)) = plain.accept().await else { continue };
            tokio::spawn(async move {
                let _ = tcp.write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n").await;
            });
        }
    });
    let err = within("рукопожатие", try_connect(addr, "localhost", &ca.cert_pem())).await.expect_err("сервер не говорит по TLS");
    assert!(matches!(classify(&err), VerifyFailure::Other(_)), "чужой протокол это не про сертификаты: {:?}", classify(&err));
}

#[test]
fn descriptions_differ_and_help() {
    let all = [
        VerifyFailure::UnknownIssuer,
        VerifyFailure::NameMismatch,
        VerifyFailure::Expired,
        VerifyFailure::NotYetValid,
        VerifyFailure::Other("обрыв".to_string()),
    ];
    let texts: Vec<String> = all.iter().map(describe).collect();
    for (i, a) in texts.iter().enumerate() {
        assert!(a.chars().count() > 15, "описание {a:?} слишком короткое: оно должно говорить, что делать");
        for b in &texts[i + 1..] {
            assert_ne!(a, b, "у разных причин должны быть разные описания");
        }
    }
    assert!(texts[4].contains("обрыв"), "для Other в описании нужен исходный текст ошибки");
}
