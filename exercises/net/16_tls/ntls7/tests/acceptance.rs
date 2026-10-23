use ntls7::{client_config, TrustError};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_rustls::{TlsAcceptor, TlsConnector};

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

async fn handshake(trusted: &[String], leaf: &Leaf, name: &str) -> Result<(), String> {
    let config = client_config(trusted).map_err(|e| format!("{e:?}"))?;
    let (client_io, server_io) = tokio::io::duplex(64 * 1024);
    let acceptor = TlsAcceptor::from(kit_server_config(&leaf.cert_pem, &leaf.key_pem));
    let server = tokio::spawn(async move {
        if let Ok(mut tls) = acceptor.accept(server_io).await {
            let _ = tls.write_all(b"hi").await;
            let _ = tls.shutdown().await;
        }
    });
    let result = async {
        let mut tls = TlsConnector::from(config).connect(server_name(name), client_io).await.map_err(|e| e.to_string())?;
        let mut text = Vec::new();
        tls.read_to_end(&mut text).await.map_err(|e| e.to_string())?;
        Ok(())
    };
    let outcome = within("рукопожатие", result).await;
    let _ = server.await;
    outcome
}

#[tokio::test]
async fn trusted_ca_connects() {
    let ca = Ca::new("мой CA");
    let leaf = ca.issue(&["localhost", "127.0.0.1"], Usage::Server);
    handshake(&[ca.cert_pem()], &leaf, "localhost").await.expect("сервер с сертификатом доверенного CA");
    handshake(&[ca.cert_pem()], &leaf, "127.0.0.1").await.expect("по IP");
}

#[tokio::test]
async fn man_in_the_middle_is_rejected() {
    let ca = Ca::new("мой CA");
    let attacker = Ca::new("CA злоумышленника");
    let fake = attacker.issue(&["localhost"], Usage::Server);
    let err = handshake(&[ca.cert_pem()], &fake, "localhost").await.expect_err("сертификат от чужого CA: клиент обязан отказаться");
    assert!(err.contains("UnknownIssuer"), "ждали UnknownIssuer, получили {err}");
}

#[tokio::test]
async fn wrong_name_is_rejected() {
    let ca = Ca::new("мой CA");
    let leaf = ca.issue(&["billing.example.test"], Usage::Server);
    let err = handshake(&[ca.cert_pem()], &leaf, "localhost").await.expect_err("сертификат выдан на другое имя");
    assert!(err.contains("not valid for name"), "ждали ошибку про имя, получили {err}");
}

#[tokio::test]
async fn expired_certificate_is_rejected() {
    let ca = Ca::new("мой CA");
    let leaf = ca.issue_valid(&["localhost"], Usage::Server, (2000, 1, 1), (2001, 1, 1));
    let err = handshake(&[ca.cert_pem()], &leaf, "localhost").await.expect_err("сертификат просрочен");
    assert!(err.contains("expired"), "ждали ошибку про срок, получили {err}");
}

#[tokio::test]
async fn several_trusted_cas_work_during_rotation() {
    let old = Ca::new("старый CA");
    let new = Ca::new("новый CA");
    let leaf_new = new.issue(&["localhost"], Usage::Server);
    let leaf_old = old.issue(&["localhost"], Usage::Server);
    let both = [old.cert_pem(), new.cert_pem()];
    handshake(&both, &leaf_new, "localhost").await.expect("сертификат нового CA");
    handshake(&both, &leaf_old, "localhost").await.expect("сертификат старого CA");
    let third = Ca::new("третий CA").issue(&["localhost"], Usage::Server);
    handshake(&both, &third, "localhost").await.expect_err("третий CA не в списке");
}

#[test]
fn empty_and_broken_trust_lists_are_errors() {
    assert_eq!(client_config(&[]).err(), Some(TrustError::Empty), "пустой список доверия это ошибка");
    assert_eq!(client_config(&["просто текст".to_string()]).err(), Some(TrustError::Empty), "в тексте нет ни одного сертификата");
    let broken = "-----BEGIN CERTIFICATE-----\nне base64!\n-----END CERTIFICATE-----\n".to_string();
    assert!(matches!(client_config(&[broken]), Err(TrustError::BadPem(_))), "повреждённый сертификат это BadPem");
}
