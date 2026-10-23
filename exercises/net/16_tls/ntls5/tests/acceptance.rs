use ntls5::tap::Tap;
use ntls5::{contains, looks_like_tls, sni};
use std::net::SocketAddr;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::{TlsAcceptor, TlsConnector};

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

async fn plain_login_server() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let Ok((tcp, _)) = listener.accept().await else { continue };
            tokio::spawn(async move {
                let mut reader = BufReader::new(tcp);
                let mut line = String::new();
                if reader.read_line(&mut line).await.is_ok() {
                    let _ = reader.get_mut().write_all(b"OK\n").await;
                }
            });
        }
    });
    addr
}

async fn tls_login_server(ca: &Ca) -> SocketAddr {
    let leaf = ca.issue(&["localhost", "127.0.0.1"], Usage::Server);
    let acceptor = TlsAcceptor::from(kit_server_config(&leaf.cert_pem, &leaf.key_pem));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let Ok((tcp, _)) = listener.accept().await else { continue };
            let acceptor = acceptor.clone();
            tokio::spawn(async move {
                let Ok(tls) = acceptor.accept(tcp).await else { return };
                let mut reader = BufReader::new(tls);
                let mut line = String::new();
                if reader.read_line(&mut line).await.is_ok() {
                    let _ = reader.get_mut().write_all(b"OK\n").await;
                    let _ = reader.get_mut().shutdown().await;
                }
            });
        }
    });
    addr
}

async fn login_plain(addr: SocketAddr) {
    let mut tcp = TcpStream::connect(addr).await.unwrap();
    tcp.write_all(b"LOGIN ann hunter2\n").await.unwrap();
    let mut reply = String::new();
    BufReader::new(tcp).read_line(&mut reply).await.unwrap();
    assert_eq!(reply, "OK\n");
}

async fn login_tls(addr: SocketAddr, ca_pem: &str, name: &str) {
    let tcp = TcpStream::connect(addr).await.unwrap();
    let mut tls = TlsConnector::from(kit_client_config(ca_pem)).connect(server_name(name), tcp).await.expect("рукопожатие через прослушку");
    tls.write_all(b"LOGIN ann hunter2\n").await.unwrap();
    let mut reply = Vec::new();
    tls.read_to_end(&mut reply).await.ok();
    assert_eq!(reply, b"OK\n");
}

#[tokio::test]
async fn plain_tcp_shows_the_password_to_the_listener() {
    let server = plain_login_server().await;
    let tap = Tap::start(server).await;
    within("вход", login_plain(tap.addr)).await;
    let seen = tap.captured_to_server();
    assert!(contains(&seen, b"hunter2"), "пароль должен быть виден в записи обычного TCP");
    assert!(contains(&seen, b"LOGIN ann"));
    assert!(!looks_like_tls(&seen), "это обычный текст, а не TLS");
    assert!(!contains(&seen, b"hunter3") && !contains(&seen, b"hunter22"), "contains нашёл то, чего нет");
    assert!(contains(&seen, b""), "пустая иголка найдена всегда");
    assert!(!contains(b"ab", b"abc"), "иголка длиннее стога");
    assert_eq!(sni(&seen), None, "в открытом тексте нет ClientHello");
}

#[tokio::test]
async fn tls_hides_the_password_but_not_the_server_name() {
    let ca = Ca::new("тестовый CA");
    let server = tls_login_server(&ca).await;
    let tap = Tap::start(server).await;
    within("вход", login_tls(tap.addr, &ca.cert_pem(), "localhost")).await;
    for (direction, seen) in [("клиент → сервер", tap.captured_to_server()), ("сервер → клиент", tap.captured_to_client())] {
        assert!(!contains(&seen, b"hunter2"), "{direction}: пароль виден в записи TLS");
        assert!(!contains(&seen, b"LOGIN"), "{direction}: команда видна в записи TLS");
        assert!(!contains(&seen, b"OK\n"), "{direction}: ответ виден в записи TLS");
    }
    let hello = tap.captured_to_server();
    assert!(looks_like_tls(&hello), "запись TLS должна узнаваться по заголовку");
    assert_eq!(sni(&hello), Some("localhost".to_string()), "имя сервера идёт в ClientHello открытым текстом: слушатель видит, куда ты идёшь");
}

#[tokio::test]
async fn connecting_by_ip_sends_no_server_name() {
    let ca = Ca::new("тестовый CA");
    let server = tls_login_server(&ca).await;
    let tap = Tap::start(server).await;
    within("вход", login_tls(tap.addr, &ca.cert_pem(), "127.0.0.1")).await;
    let hello = tap.captured_to_server();
    assert!(looks_like_tls(&hello));
    assert_eq!(sni(&hello), None, "для IP-адреса клиент не отправляет SNI");
}

#[tokio::test]
async fn truncated_hello_never_panics_and_never_lies() {
    let ca = Ca::new("тестовый CA");
    let server = tls_login_server(&ca).await;
    for name in ["localhost", "127.0.0.1"] {
        let tap = Tap::start(server).await;
        within("вход", login_tls(tap.addr, &ca.cert_pem(), name)).await;
        let hello = tap.captured_to_server();
        let expected = sni(&hello);
        for cut in 0..hello.len().min(600) {
            let got = sni(&hello[..cut]);
            assert!(got.is_none() || got == expected, "на обрезке до {cut} байт разбор вернул {got:?}, а в полном ClientHello {expected:?}");
        }
    }
}

#[test]
fn junk_input_is_not_a_hello() {
    for bytes in [&b""[..], b"GET / HTTP/1.1\r\n\r\n", &[22u8][..], &[22, 3, 1, 0, 5, 1, 0, 0, 1, 3][..], &[0xff; 64][..]] {
        assert_eq!(sni(bytes), None, "на {bytes:?} имени быть не должно");
    }
    assert!(!looks_like_tls(b"GET /"));
    assert!(!looks_like_tls(b""));
    assert!(looks_like_tls(&[22, 3, 1, 0, 200]));
}
