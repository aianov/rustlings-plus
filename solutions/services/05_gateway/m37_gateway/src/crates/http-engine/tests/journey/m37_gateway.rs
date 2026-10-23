use std::net::SocketAddr;
use std::time::Duration;

use http_engine::{
    PROXY_LINE_MAX, ProxyLine, Request, Response, Server, ServerConfig, handler, parse_proxy_line,
    proxy_line,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
use tokio::time::timeout;

const WAIT: Duration = Duration::from_secs(5);

#[test]
fn proxy_lines_are_parsed_strictly() {
    let line = b"PROXY TCP4 203.0.113.7 10.0.0.1 51000 443\r\nGET / HTTP/1.1\r\n";
    assert_eq!(
        parse_proxy_line(line),
        ProxyLine::Peer {
            addr: "203.0.113.7:51000".parse().expect("адрес"),
            consumed: 43,
        },
        "из строки PROXY берётся адрес и порт клиента, а остаток буфера это уже HTTP"
    );
    assert_eq!(
        parse_proxy_line(b"PROXY TCP6 2001:db8::1 2001:db8::2 4000 443\r\n"),
        ProxyLine::Peer {
            addr: "[2001:db8::1]:4000".parse().expect("адрес"),
            consumed: 45,
        }
    );
    assert_eq!(
        parse_proxy_line(b"PROXY UNKNOWN\r\n"),
        ProxyLine::Unknown { consumed: 15 },
        "UNKNOWN честно говорит, что адреса нет"
    );
    assert_eq!(
        parse_proxy_line(b"PROXY TCP4 1.2.3.4"),
        ProxyLine::Incomplete
    );
    assert_eq!(parse_proxy_line(b"PRO"), ProxyLine::Incomplete);
    for broken in [
        &b"GET / HTTP/1.1\r\n"[..],
        b"PROXY TCP4 1.2.3.400 10.0.0.1 1 2\r\n",
        b"PROXY TCP4 2001:db8::1 10.0.0.1 1 2\r\n",
        b"PROXY TCP4 1.2.3.4 10.0.0.1 70000 2\r\n",
        b"PROXY TCP4 1.2.3.4 10.0.0.1 1\r\n",
    ] {
        assert_eq!(
            parse_proxy_line(broken),
            ProxyLine::Broken,
            "{:?}",
            String::from_utf8_lossy(broken)
        );
    }
    let endless = [b'P'; PROXY_LINE_MAX + 1];
    let mut long = b"PROXY ".to_vec();
    long.extend_from_slice(&endless);
    assert_eq!(
        parse_proxy_line(&long),
        ProxyLine::Broken,
        "строка длиннее {PROXY_LINE_MAX} байт без конца это не PROXY, а мусор"
    );
    let client: SocketAddr = "198.51.100.4:6000".parse().expect("адрес");
    let local: SocketAddr = "127.0.0.1:443".parse().expect("адрес");
    let written = proxy_line(client, local);
    assert_eq!(written, "PROXY TCP4 198.51.100.4 127.0.0.1 6000 443\r\n");
    assert_eq!(
        parse_proxy_line(written.as_bytes()),
        ProxyLine::Peer {
            addr: client,
            consumed: written.len()
        }
    );
}

async fn echo_peer(proxy_protocol: bool) -> (SocketAddr, oneshot::Sender<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("порт");
    let addr = listener.local_addr().expect("адрес");
    let app = handler(|request: Request| async move {
        let peer = request
            .peer
            .map_or(String::from("-"), |peer| peer.to_string());
        Response::text(200, &peer)
    });
    let config = ServerConfig {
        proxy_protocol,
        ..ServerConfig::default()
    };
    let (stop, stopped) = oneshot::channel::<()>();
    tokio::spawn(async move {
        let _ = Server::with_handler(app)
            .config(config)
            .serve(listener, async move {
                let _ = stopped.await;
            })
            .await;
    });
    (addr, stop)
}

async fn ask(addr: SocketAddr, prefix: &str) -> (String, SocketAddr) {
    let mut tcp = TcpStream::connect(addr).await.expect("соединение");
    let local = tcp.local_addr().expect("свой адрес");
    let request = format!("{prefix}GET / HTTP/1.1\r\nHost: t\r\nConnection: close\r\n\r\n");
    tcp.write_all(request.as_bytes()).await.expect("пишется");
    let mut answer = String::new();
    let _ = timeout(WAIT, tcp.read_to_string(&mut answer)).await;
    (answer, local)
}

#[tokio::test]
async fn the_server_knows_who_connected() {
    let (plain, _stop) = echo_peer(false).await;
    let (answer, local) = ask(plain, "").await;
    assert!(
        answer.ends_with(&local.to_string()),
        "без PROXY адрес клиента это адрес TCP-соединения: {answer}"
    );
    let (behind, _stop) = echo_peer(true).await;
    let (answer, _) = ask(behind, "PROXY TCP4 203.0.113.9 127.0.0.1 40000 443\r\n").await;
    assert!(
        answer.starts_with("HTTP/1.1 200") && answer.ends_with("203.0.113.9:40000"),
        "за своим TLS-краем адрес клиента приходит строкой PROXY: {answer}"
    );
    let (answer, _) = ask(behind, "PROXY UNKNOWN\r\n").await;
    assert!(answer.ends_with("\r\n-"), "UNKNOWN: адреса нет: {answer}");
    let (answer, _) = ask(behind, "").await;
    assert!(
        answer.is_empty(),
        "сервер, который ждёт PROXY, закрывает соединение без неё: {answer:?}"
    );
}
