use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use rustls::pki_types::ServerName;
use service_runtime::CancellationToken;
use tls_edge::{client_config, server_config, terminate};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::timeout;
use tokio_rustls::TlsConnector;

const WAIT: Duration = Duration::from_secs(5);

fn certs(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("m32-edge")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    certgen::issue_all(&dir, false).expect("сертификаты выпускаются");
    dir
}

async fn echo() -> (std::net::SocketAddr, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("порт");
    let addr = listener.local_addr().expect("адрес");
    let seen = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&seen);
    tokio::spawn(async move {
        while let Ok((mut tcp, _)) = listener.accept().await {
            counter.fetch_add(1, Ordering::SeqCst);
            tokio::spawn(async move {
                let mut buf = [0u8; 1024];
                while let Ok(n) = tcp.read(&mut buf).await {
                    if n == 0 || tcp.write_all(&buf[..n]).await.is_err() {
                        break;
                    }
                }
            });
        }
    });
    (addr, seen)
}

#[tokio::test]
async fn edge_decrypts_and_relays_to_the_plain_engine() {
    let dir = certs("relay");
    let read = |file: &str| fs::read(dir.join(file)).expect("файл сертификата");
    let (inner, seen) = echo().await;
    let public = TcpListener::bind("127.0.0.1:0").await.expect("порт");
    let addr = public.local_addr().expect("адрес");
    let stop = CancellationToken::new();
    let config = server_config(&read("api-gateway.pem"), &read("api-gateway.key")).expect("конфиг");
    let edge = tokio::spawn(terminate(public, inner, config, stop.clone()));

    let trusted = TlsConnector::from(client_config(&read("ca.pem")).expect("клиент"));
    let tcp = TcpStream::connect(addr).await.expect("соединение");
    let name = ServerName::try_from("localhost").expect("имя");
    let mut tls = trusted
        .connect(name, tcp)
        .await
        .expect("рукопожатие с нашим CA");
    assert_eq!(
        tls.get_ref().1.alpn_protocol(),
        Some(&b"http/1.1"[..]),
        "терминатор договаривается о http/1.1"
    );
    tls.write_all(b"ping").await.expect("пишется");
    let mut back = [0u8; 4];
    timeout(WAIT, tls.read_exact(&mut back))
        .await
        .expect("ответ")
        .expect("читается");
    assert_eq!(
        &back, b"ping",
        "байты дошли до движка открытым текстом и вернулись шифрованными"
    );

    let other = certs("stranger");
    let stranger = TlsConnector::from(
        client_config(&fs::read(other.join("ca.pem")).expect("чужой CA")).expect("клиент"),
    );
    let tcp = TcpStream::connect(addr).await.expect("соединение");
    let name = ServerName::try_from("localhost").expect("имя");
    assert!(
        stranger.connect(name, tcp).await.is_err(),
        "клиент, который доверяет другому CA, не принимает наш сертификат"
    );

    let mut plain = TcpStream::connect(addr).await.expect("соединение");
    plain
        .write_all(b"GET /health HTTP/1.1\r\nHost: x\r\n\r\n")
        .await
        .expect("пишется");
    let mut answer = Vec::new();
    let _ = timeout(WAIT, plain.read_to_end(&mut answer)).await;
    assert!(
        !String::from_utf8_lossy(&answer).contains("HTTP/1.1"),
        "открытый текст на TLS-порт не доходит до движка"
    );
    assert_eq!(
        seen.load(Ordering::SeqCst),
        1,
        "до движка дошло только одно, честное соединение"
    );
    stop.cancel();
    drop(tls);
    timeout(WAIT, edge)
        .await
        .expect("терминатор останавливается")
        .expect("задача")
        .expect("без ошибки");
}
