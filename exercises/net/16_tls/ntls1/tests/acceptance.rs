use ntls1::{write_pems, Ca};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_rustls::{TlsAcceptor, TlsConnector};

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

async fn handshake(ca_pem: &str, cert_pem: &str, key_pem: &str, name: &str) -> Result<(), String> {
    let (client_io, server_io) = tokio::io::duplex(64 * 1024);
    let acceptor = TlsAcceptor::from(kit_server_config(cert_pem, key_pem));
    let connector = TlsConnector::from(kit_client_config(ca_pem));
    let server = tokio::spawn(async move {
        let mut tls = acceptor.accept(server_io).await.map_err(|e| e.to_string())?;
        tls.write_all(b"hello").await.map_err(|e| e.to_string())?;
        tls.shutdown().await.map_err(|e| e.to_string())
    });
    let mut tls = within("клиентское рукопожатие", connector.connect(server_name(name), client_io)).await.map_err(|e| e.to_string())?;
    let mut text = Vec::new();
    tls.read_to_end(&mut text).await.map_err(|e| e.to_string())?;
    let _ = server.await;
    assert_eq!(text, b"hello");
    Ok(())
}

fn leaf_for(ca: &Ca, names: &[&str]) -> (String, String) {
    let leaf = ca.issue_server(names).expect("сертификат сервера");
    (leaf.cert_pem, leaf.key_pem)
}

#[tokio::test]
async fn trusted_server_cert_works_for_dns_name_and_ip() {
    let ca = Ca::new("тестовый CA").expect("CA");
    let (cert, key) = leaf_for(&ca, &["localhost", "127.0.0.1"]);
    handshake(&ca.cert_pem(), &cert, &key, "localhost").await.expect("по имени localhost должно получиться");
    handshake(&ca.cert_pem(), &cert, &key, "127.0.0.1").await.expect("по IP 127.0.0.1 должно получиться: адрес должен быть в SAN как IP, а не как DNS-имя");
}

#[tokio::test]
async fn certificate_is_valid_only_for_its_own_names() {
    let ca = Ca::new("тестовый CA").expect("CA");
    let (cert, key) = leaf_for(&ca, &["api.example.test"]);
    handshake(&ca.cert_pem(), &cert, &key, "api.example.test").await.expect("своё имя должно подходить");
    let err = handshake(&ca.cert_pem(), &cert, &key, "localhost").await.expect_err("чужое имя не должно подходить");
    assert!(err.contains("not valid for name") || err.contains("NotValidForName"), "ждали ошибку про несовпадение имени, получили: {err}");
}

#[tokio::test]
async fn foreign_ca_is_not_trusted() {
    let mine = Ca::new("мой CA").expect("CA");
    let other = Ca::new("мой CA").expect("CA");
    let (cert, key) = leaf_for(&mine, &["localhost"]);
    let err = handshake(&other.cert_pem(), &cert, &key, "localhost").await.expect_err("два CA с одинаковым именем это разные CA");
    assert!(err.contains("UnknownIssuer") || err.contains("BadSignature"), "ждали UnknownIssuer или BadSignature, получили: {err}");
}

fn parsed<R>(pem: &str, f: impl FnOnce(&x509_parser::certificate::X509Certificate) -> R) -> R {
    let der = certs_of(pem).remove(0);
    let (_, cert) = x509_parser::parse_x509_certificate(der.as_ref()).expect("разбор сертификата");
    f(&cert)
}

#[test]
fn ca_is_a_ca_and_the_server_certificate_is_not() {
    let ca = Ca::new("корень").expect("CA");
    let leaf = ca.issue_server(&["localhost"]).expect("сертификат");
    let ca_flag = parsed(&ca.cert_pem(), |c| c.basic_constraints().ok().flatten().map(|b| b.value.ca));
    assert_eq!(ca_flag, Some(true), "у CA в basicConstraints должно быть CA:TRUE");
    let leaf_flag = parsed(&leaf.cert_pem, |c| c.basic_constraints().ok().flatten().map(|b| b.value.ca));
    assert_ne!(leaf_flag, Some(true), "сертификат сервера не должен быть CA: иначе его ключ сможет выпускать любые сертификаты");
}

#[test]
fn san_lists_dns_names_and_ips_separately() {
    use x509_parser::extensions::GeneralName;
    let ca = Ca::new("корень").expect("CA");
    let leaf = ca.issue_server(&["localhost", "api.example.test", "127.0.0.1"]).expect("сертификат");
    let (dns, ips) = parsed(&leaf.cert_pem, |c| {
        let san = c.subject_alternative_name().ok().flatten().expect("в сертификате нет SAN");
        let mut dns = Vec::new();
        let mut ips = Vec::new();
        for name in &san.value.general_names {
            match name {
                GeneralName::DNSName(n) => dns.push(n.to_string()),
                GeneralName::IPAddress(bytes) => ips.push(bytes.to_vec()),
                _ => {}
            }
        }
        (dns, ips)
    });
    assert_eq!(dns, vec!["localhost", "api.example.test"]);
    assert_eq!(ips, vec![vec![127, 0, 0, 1]]);
}

#[test]
fn pem_blocks_look_right() {
    let ca = Ca::new("корень").expect("CA");
    let leaf = ca.issue_server(&["localhost"]).expect("сертификат");
    assert!(ca.cert_pem().starts_with("-----BEGIN CERTIFICATE-----"));
    assert!(leaf.cert_pem.starts_with("-----BEGIN CERTIFICATE-----"));
    assert!(leaf.key_pem.contains("PRIVATE KEY-----"), "ключ должен быть в PEM: {}", leaf.key_pem.lines().next().unwrap_or(""));
    assert!(!leaf.cert_pem.contains("PRIVATE KEY"), "в файле сертификата не должно быть ключа");
}

#[test]
fn files_are_written_with_private_key_protected() {
    let ca = Ca::new("корень").expect("CA");
    let leaf = ca.issue_server(&["localhost"]).expect("сертификат");
    let dir = std::env::temp_dir().join(format!("ntls1-{}-{:?}", std::process::id(), std::thread::current().id()).replace(['(', ')'], ""));
    let _ = std::fs::remove_dir_all(&dir);
    write_pems(&dir, &ca, &leaf).expect("запись файлов");
    let mut names: Vec<String> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().to_string()).collect();
    names.sort();
    assert_eq!(names, vec!["ca.pem", "server.key", "server.pem"], "в каталоге должны лежать ровно эти три файла (закрытого ключа CA среди них нет)");
    assert_eq!(std::fs::read_to_string(dir.join("ca.pem")).unwrap(), ca.cert_pem());
    assert_eq!(std::fs::read_to_string(dir.join("server.key")).unwrap(), leaf.key_pem);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(dir.join("server.key")).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode & 0o077, 0, "закрытый ключ доступен другим пользователям (права {mode:o}): создавай файл сразу с правами 0600");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
