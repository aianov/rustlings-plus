use ntls2::{load_certs, load_key, server_config, TlsError};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_rustls::{TlsAcceptor, TlsConnector};
use std::sync::Arc;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, ServerName};
use rustls::{ClientConfig, RootCertStore};

#[path = "../src/pki.rs"]
mod pki;
use pki::{Ca, Usage};

async fn within<T>(what: &str, f: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(std::time::Duration::from_secs(5), f)
        .await
        .unwrap_or_else(|_| panic!("{what}: не дождались ответа за 5 секунд"))
}

fn sample() -> (Ca, pki::Leaf) {
    let ca = Ca::new("тестовый CA");
    let leaf = ca.issue(&["localhost"], Usage::Server);
    (ca, leaf)
}

#[test]
fn chain_is_read_in_file_order() {
    let (ca, leaf) = sample();
    let file = format!("{}{}", leaf.cert_pem, ca.cert_pem());
    let certs = load_certs(file.as_bytes()).expect("цепочка из двух сертификатов");
    assert_eq!(certs.len(), 2, "в файле два сертификата");
    let first = CertificateDer::from_pem_slice(leaf.cert_pem.as_bytes()).unwrap();
    assert_eq!(certs[0].as_ref(), first.as_ref(), "первым должен идти сертификат сервера, порядок файла сохраняется");
}

#[test]
fn nothing_usable_is_no_certificates() {
    let (_, leaf) = sample();
    for (what, bytes) in [("пустой файл", String::new()), ("мусор", "это вообще не PEM, а записка".to_string()), ("только ключ", leaf.key_pem.clone())] {
        assert_eq!(load_certs(bytes.as_bytes()).unwrap_err(), TlsError::NoCertificates, "{what}: сертификатов нет");
    }
}

#[test]
fn damaged_block_is_bad_pem_not_silence() {
    let (_, leaf) = sample();
    let cut = &leaf.cert_pem[..leaf.cert_pem.len() / 2];
    assert!(matches!(load_certs(cut.as_bytes()), Err(TlsError::BadPem(_))), "обрезанный сертификат это повреждённый файл, а не пустой");
    let broken = leaf.cert_pem.replacen("MII", "M!!", 1);
    assert!(matches!(load_certs(broken.as_bytes()), Err(TlsError::BadPem(_))), "плохой base64 это повреждённый файл");
    let cut_key = &leaf.key_pem[..leaf.key_pem.len() / 2];
    assert!(matches!(load_key(cut_key.as_bytes()), Err(TlsError::BadPem(_))), "обрезанный ключ это повреждённый файл");
}

#[test]
fn key_loading_tells_missing_from_present() {
    let (_, leaf) = sample();
    assert!(load_key(leaf.key_pem.as_bytes()).is_ok());
    for (what, bytes) in [("пустой файл", String::new()), ("только сертификат", leaf.cert_pem.clone())] {
        assert_eq!(load_key(bytes.as_bytes()).unwrap_err(), TlsError::NoPrivateKey, "{what}: ключа нет");
    }
}

#[test]
fn foreign_key_is_a_key_mismatch() {
    let ca = Ca::new("тестовый CA");
    let a = ca.issue(&["localhost"], Usage::Server);
    let b = ca.issue(&["localhost"], Usage::Server);
    let Err(err) = server_config(a.cert_pem.as_bytes(), b.key_pem.as_bytes()) else {
        panic!("ключ от другого сертификата");
    };
    assert!(matches!(err, TlsError::KeyMismatch(_)), "ждали KeyMismatch, получили {err:?}");
}

#[test]
fn errors_are_readable_by_a_human() {
    assert!(TlsError::NoCertificates.to_string().contains("сертификат"));
    assert!(TlsError::NoPrivateKey.to_string().contains("ключ"));
}

#[tokio::test]
async fn built_config_really_serves_a_handshake() {
    let (ca, leaf) = sample();
    let file = format!("{}{}", leaf.cert_pem, ca.cert_pem());
    let config = server_config(file.as_bytes(), leaf.key_pem.as_bytes()).expect("конфигурация");
    let mut roots = RootCertStore::empty();
    roots.add(CertificateDer::from_pem_slice(ca.cert_pem().as_bytes()).unwrap()).unwrap();
    let client = Arc::new(ClientConfig::builder().with_root_certificates(roots).with_no_client_auth());
    let (c, s) = tokio::io::duplex(64 * 1024);
    let server = tokio::spawn(async move {
        let mut tls = TlsAcceptor::from(config).accept(s).await.unwrap();
        tls.write_all(b"ok").await.unwrap();
        tls.shutdown().await.unwrap();
    });
    let mut tls = within("рукопожатие", TlsConnector::from(client).connect(ServerName::try_from("localhost").unwrap(), c)).await.expect("рукопожатие");
    let mut text = Vec::new();
    tls.read_to_end(&mut text).await.unwrap();
    assert_eq!(text, b"ok");
    server.await.unwrap();
}
