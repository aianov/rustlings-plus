use ntls9::{negotiate, server_config};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;

#[path = "../src/pki.rs"]
mod pki;
use pki::{Ca, Usage};

async fn within<T>(what: &str, f: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(std::time::Duration::from_secs(5), f)
        .await
        .unwrap_or_else(|_| panic!("{what}: не дождались ответа за 5 секунд"))
}

type Seen = Arc<Mutex<Vec<Option<String>>>>;

async fn start(ca: &Ca) -> (SocketAddr, Seen) {
    let leaf = ca.issue(&["localhost"], Usage::Server);
    let acceptor = TlsAcceptor::from(server_config(&leaf.cert_pem, &leaf.key_pem));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let seen: Seen = Arc::default();
    let record = seen.clone();
    tokio::spawn(async move {
        loop {
            let Ok((tcp, _)) = listener.accept().await else { continue };
            let acceptor = acceptor.clone();
            let record = record.clone();
            tokio::spawn(async move {
                let Ok(mut tls) = acceptor.accept(tcp).await else { return };
                let chosen = tls.get_ref().1.alpn_protocol().map(|p| String::from_utf8_lossy(p).into_owned());
                record.lock().unwrap().push(chosen);
                let _ = tls.shutdown().await;
            });
        }
    });
    (addr, seen)
}

async fn run(offer: &[&str]) -> (std::io::Result<Option<String>>, Vec<Option<String>>) {
    let ca = Ca::new("тестовый CA");
    let (addr, seen) = start(&ca).await;
    let result = within("рукопожатие", negotiate(addr, "localhost", &ca.cert_pem(), offer)).await;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let server_view = seen.lock().unwrap().clone();
    (result, server_view)
}

#[tokio::test]
async fn client_that_knows_both_gets_h2() {
    let (result, server) = run(&["h2", "http/1.1"]).await;
    assert_eq!(result.unwrap(), Some("h2".to_string()));
    assert_eq!(server, vec![Some("h2".to_string())], "сервер должен видеть тот же итог");
}

#[tokio::test]
async fn http1_only_client_gets_http1() {
    let (result, server) = run(&["http/1.1"]).await;
    assert_eq!(result.unwrap(), Some("http/1.1".to_string()));
    assert_eq!(server, vec![Some("http/1.1".to_string())]);
}

#[tokio::test]
async fn server_preference_wins_over_client_order() {
    let (result, _) = run(&["http/1.1", "h2"]).await;
    assert_eq!(result.unwrap(), Some("h2".to_string()), "выбирает сервер: он предпочитает h2, хотя клиент назвал его вторым");
}

#[tokio::test]
async fn client_without_alpn_connects_with_no_protocol() {
    let (result, server) = run(&[]).await;
    assert_eq!(result.expect("клиент без ALPN должен подключаться"), None);
    assert_eq!(server, vec![None]);
}

#[tokio::test]
async fn client_with_foreign_protocol_is_refused() {
    let (result, server) = run(&["spdy/3"]).await;
    let err = result.expect_err("общих протоколов нет: рукопожатие должно оборваться");
    assert!(!err.to_string().is_empty());
    assert!(server.is_empty(), "сервер не должен считать такое соединение установленным: {server:?}");
}
