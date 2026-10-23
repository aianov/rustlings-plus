use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use rustls::pki_types::ServerName;
use service_runtime::CancellationToken;
use tls_edge::{
    client_config, mutual_client_config, mutual_server_config, server_config, terminate,
    terminate_announced,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::time::timeout;
use tokio_rustls::TlsConnector;

const WAIT: Duration = Duration::from_secs(5);

fn certs(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("m37-edge")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    certgen::issue_all(&dir, false).expect("сертификаты выпускаются");
    dir
}

async fn recorder() -> (SocketAddr, mpsc::Receiver<Vec<u8>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("порт");
    let addr = listener.local_addr().expect("адрес");
    let (tx, rx) = mpsc::channel(8);
    tokio::spawn(async move {
        while let Ok((mut tcp, _)) = listener.accept().await {
            let tx = tx.clone();
            tokio::spawn(async move {
                let mut buf = [0u8; 1024];
                while let Ok(n) = tcp.read(&mut buf).await {
                    if n == 0 || tx.send(buf[..n].to_vec()).await.is_err() {
                        break;
                    }
                }
            });
        }
    });
    (addr, rx)
}

async fn received(rx: &mut mpsc::Receiver<Vec<u8>>, want: usize) -> String {
    let mut all = Vec::new();
    while all.len() < want {
        let chunk = timeout(WAIT, rx.recv())
            .await
            .expect("байты дошли до движка")
            .expect("канал жив");
        all.extend(chunk);
    }
    String::from_utf8_lossy(&all).to_string()
}

#[tokio::test]
async fn the_announcing_edge_tells_the_engine_who_connected() {
    let dir = certs("announce");
    let read = |file: &str| fs::read(dir.join(file)).expect("файл сертификата");
    let (inner, mut seen) = recorder().await;
    let public = TcpListener::bind("127.0.0.1:0").await.expect("порт");
    let addr = public.local_addr().expect("адрес");
    let stop = CancellationToken::new();
    let config = server_config(&read("api-gateway.pem"), &read("api-gateway.key")).expect("конфиг");
    let edge = tokio::spawn(terminate_announced(public, inner, config, stop.clone()));
    let connector = TlsConnector::from(client_config(&read("ca.pem")).expect("клиент"));
    let tcp = TcpStream::connect(addr).await.expect("соединение");
    let client = tcp.local_addr().expect("адрес клиента");
    let name = ServerName::try_from("localhost").expect("имя");
    let mut tls = connector.connect(name, tcp).await.expect("рукопожатие");
    tls.write_all(b"GET / HTTP/1.1\r\n").await.expect("пишется");
    let expected = format!(
        "PROXY TCP4 {} {} {} {}\r\nGET / HTTP/1.1\r\n",
        client.ip(),
        addr.ip(),
        client.port(),
        addr.port()
    );
    let got = received(&mut seen, expected.len()).await;
    assert_eq!(
        got, expected,
        "край пишет движку строку PROXY с адресом клиента, потом расшифрованные байты"
    );
    stop.cancel();
    drop(tls);
    timeout(WAIT, edge)
        .await
        .expect("край останавливается")
        .expect("задача")
        .expect("без ошибки");
}

#[tokio::test]
async fn mutual_tls_lets_in_only_our_services() {
    let dir = certs("mutual");
    let read = |file: &str| fs::read(dir.join(file)).expect("файл сертификата");
    let (inner, mut seen) = recorder().await;
    let public = TcpListener::bind("127.0.0.1:0").await.expect("порт");
    let addr = public.local_addr().expect("адрес");
    let stop = CancellationToken::new();
    let config = mutual_server_config(
        &read("auth-service.pem"),
        &read("auth-service.key"),
        &read("ca.pem"),
    )
    .expect("конфиг");
    let edge = tokio::spawn(terminate(public, inner, config, stop.clone()));
    let name = || ServerName::try_from("auth-service").expect("имя");

    let anonymous = TlsConnector::from(client_config(&read("ca.pem")).expect("клиент"));
    let tcp = TcpStream::connect(addr).await.expect("соединение");
    let refused = match anonymous.connect(name(), tcp).await {
        Err(..) => true,
        Ok(mut tls) => {
            let _ = tls.write_all(b"ping").await;
            let mut back = [0u8; 4];
            !matches!(timeout(WAIT, tls.read_exact(&mut back)).await, Ok(Ok(..)))
        }
    };
    assert!(
        refused,
        "клиент без сертификата не проходит: вход только для своих"
    );

    let gateway = TlsConnector::from(
        mutual_client_config(
            &read("ca.pem"),
            &read("api-gateway.pem"),
            &read("api-gateway.key"),
        )
        .expect("клиент шлюза"),
    );
    let tcp = TcpStream::connect(addr).await.expect("соединение");
    let mut tls = gateway
        .connect(name(), tcp)
        .await
        .expect("шлюз со своим сертификатом проходит");
    tls.write_all(b"ping").await.expect("пишется");
    assert_eq!(received(&mut seen, 4).await, "ping");
    stop.cancel();
    drop(tls);
    timeout(WAIT, edge)
        .await
        .expect("край останавливается")
        .expect("задача")
        .expect("без ошибки");
}
