use loadgen::{HttpTarget, run_closed, run_open};
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::time::{sleep, timeout};

const MS: fn(u64) -> Duration = Duration::from_millis;

struct Server {
    addr: SocketAddr,
    accepted: Arc<AtomicU64>,
    served: Arc<AtomicU64>,
}

async fn start(delay: Duration) -> Server {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("порт 0 свободен");
    let addr = listener.local_addr().expect("адрес слушателя");
    let accepted = Arc::new(AtomicU64::new(0));
    let served = Arc::new(AtomicU64::new(0));
    let (a, s) = (accepted.clone(), served.clone());
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else { return };
            a.fetch_add(1, Ordering::SeqCst);
            let served = s.clone();
            tokio::spawn(async move {
                let mut buf = Vec::new();
                let mut chunk = [0u8; 4096];
                loop {
                    let Some(end) = buf.windows(4).position(|w| w == b"\r\n\r\n") else {
                        let Ok(n) = stream.read(&mut chunk).await else { return };
                        if n == 0 {
                            return;
                        }
                        buf.extend_from_slice(&chunk[..n]);
                        continue;
                    };
                    let request = String::from_utf8_lossy(&buf[..end]).into_owned();
                    buf.drain(..end + 4);
                    let path = request.split_whitespace().nth(1).unwrap_or("/").to_string();
                    sleep(delay).await;
                    served.fetch_add(1, Ordering::SeqCst);
                    let reply: Vec<u8> = match path.as_str() {
                        "/fail" => b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\n\r\n".to_vec(),
                        "/big" => {
                            let body = vec![b'x'; 300_000];
                            let mut out = format!("HTTP/1.1 200 OK\r\ncontent-length: {}\r\n\r\n", body.len()).into_bytes();
                            out.extend_from_slice(&body);
                            out
                        }
                        "/split" => {
                            if stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Le").await.is_err() {
                                return;
                            }
                            let _ = stream.flush().await;
                            sleep(MS(5)).await;
                            b"ngth: 2\r\n\r\nok".to_vec()
                        }
                        _ => b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok".to_vec(),
                    };
                    if stream.write_all(&reply).await.is_err() {
                        return;
                    }
                }
            });
        }
    });
    Server { addr, accepted, served }
}

#[tokio::test]
async fn closed_loop_over_real_connections_keeps_them_alive() {
    let server = start(MS(10)).await;
    let target = Arc::new(HttpTarget::new(server.addr, "/"));
    let report = timeout(Duration::from_secs(10), run_closed(target, 3, MS(400))).await.expect("прогон не завис");
    assert_eq!(report.errors, 0, "{report:?}");
    assert_eq!(report.connections, 3);
    assert_eq!(server.accepted.load(Ordering::SeqCst), 3, "сервер принял ровно три соединения: keep-alive, а не соединение на запрос");
    assert!(report.ok >= 3, "за 400 мс сделано хотя бы по запросу на воркера: {report:?}");
    assert!(report.ok <= 3 * (400 / 10 + 1), "быстрее, чем позволяет задержка сервера, не бывает: {report:?}");
    assert!(report.p50 >= MS(10), "ответ не может прийти раньше, чем сервер выдержал паузу: {:?}", report.p50);
    assert_eq!(server.served.load(Ordering::SeqCst), report.ok, "сервер обслужил ровно столько, сколько учтено успешных");
}

#[tokio::test]
async fn error_statuses_are_errors_and_not_latencies() {
    let server = start(MS(2)).await;
    let target = Arc::new(HttpTarget::new(server.addr, "/fail"));
    let report = timeout(Duration::from_secs(10), run_closed(target, 2, MS(200))).await.expect("прогон не завис");
    assert_eq!(report.ok, 0, "503 это не успех: {report:?}");
    assert!(report.errors > 0);
    assert_eq!(report.p99, Duration::ZERO, "задержек успешных ответов нет");
}

#[tokio::test]
async fn a_long_body_and_a_split_header_are_read_whole() {
    let server = start(Duration::ZERO).await;
    for path in ["/big", "/split"] {
        let target = Arc::new(HttpTarget::new(server.addr, path));
        let report = timeout(Duration::from_secs(10), run_closed(target, 1, MS(300))).await.expect("прогон не завис");
        assert!(report.ok >= 3, "{path}: {report:?}");
        assert_eq!(report.errors, 0, "{path}: ответ должен читаться до конца, иначе следующий разбор сдвинется: {report:?}");
    }
}

#[tokio::test]
async fn an_unreachable_server_is_an_error_not_a_hang() {
    let closed = {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("порт 0 свободен");
        listener.local_addr().expect("адрес")
    };
    let target = Arc::new(HttpTarget::new(closed, "/"));
    let report = timeout(Duration::from_secs(10), run_closed(target.clone(), 2, MS(200))).await.expect("прогон не завис");
    assert_eq!((report.ok, report.connections), (0, 0));
    assert!(report.errors >= 2, "{report:?}");
    let open = timeout(Duration::from_secs(10), run_open(target, 50, MS(200))).await.expect("прогон не завис");
    assert_eq!(open.ok, 0);
    assert!(open.errors > 0, "{open:?}");
}

#[tokio::test]
async fn open_loop_over_real_connections_sends_the_planned_number() {
    let server = start(MS(5)).await;
    let target = Arc::new(HttpTarget::new(server.addr, "/"));
    let report = timeout(Duration::from_secs(10), run_open(target, 100, MS(500))).await.expect("прогон не завис");
    assert_eq!(report.ok + report.errors, 50, "сто в секунду за полсекунды: ровно пятьдесят запросов по расписанию: {report:?}");
    assert_eq!(report.errors, 0, "{report:?}");
    assert!(report.p50 >= MS(5), "{:?}", report.p50);
    assert!(report.connections >= 1 && report.connections <= 50);
}
