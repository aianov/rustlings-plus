use crate::run::Target;
use futures::future::BoxFuture;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;

pub struct HttpTarget {
    addr: SocketAddr,
    path: String,
    conns: Mutex<HashMap<u64, TcpStream>>,
    next: AtomicU64,
}

impl HttpTarget {
    pub fn new(addr: SocketAddr, path: &str) -> HttpTarget {
        HttpTarget { addr, path: path.to_string(), conns: Mutex::new(HashMap::new()), next: AtomicU64::new(0) }
    }
}

fn find(buf: &[u8], needle: &[u8]) -> Option<usize> {
    buf.windows(needle.len()).position(|w| w == needle)
}

fn content_length(head: &str) -> usize {
    head.lines()
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.trim().eq_ignore_ascii_case("content-length"))
        .and_then(|(_, value)| value.trim().parse().ok())
        .unwrap_or(0)
}

async fn exchange(stream: &mut TcpStream, path: &str) -> Result<u16, String> {
    let request = format!("GET {path} HTTP/1.1\r\nHost: loadgen\r\n\r\n");
    stream.write_all(request.as_bytes()).await.map_err(|e| format!("запись: {e}"))?;
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    let head_end = loop {
        if let Some(pos) = find(&buf, b"\r\n\r\n") {
            break pos + 4;
        }
        let n = stream.read(&mut chunk).await.map_err(|e| format!("чтение: {e}"))?;
        if n == 0 {
            return Err("сервер закрыл соединение до конца заголовка".to_string());
        }
        buf.extend_from_slice(&chunk[..n]);
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).into_owned();
    let status: u16 = head.split_whitespace().nth(1).and_then(|s| s.parse().ok()).ok_or_else(|| format!("странная строка статуса: {head:?}"))?;
    let want = head_end + content_length(&head);
    while buf.len() < want {
        let n = stream.read(&mut chunk).await.map_err(|e| format!("чтение тела: {e}"))?;
        if n == 0 {
            return Err("сервер закрыл соединение посреди тела".to_string());
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    Ok(status)
}

impl Target for HttpTarget {
    fn connect(&self) -> BoxFuture<'_, Result<u64, String>> {
        Box::pin(async move {
            let stream = timeout(Duration::from_secs(5), TcpStream::connect(self.addr)).await.map_err(|_| "таймаут подключения".to_string())?.map_err(|e| format!("подключение: {e}"))?;
            let id = self.next.fetch_add(1, Ordering::SeqCst);
            self.conns.lock().expect("соединения не отравлены").insert(id, stream);
            Ok(id)
        })
    }

    fn request(&self, conn: u64) -> BoxFuture<'_, Result<(), String>> {
        Box::pin(async move {
            let taken = self.conns.lock().expect("соединения не отравлены").remove(&conn);
            let mut stream = taken.ok_or_else(|| "соединение потеряно".to_string())?;
            let status = timeout(Duration::from_secs(5), exchange(&mut stream, &self.path)).await.map_err(|_| "таймаут ответа".to_string())??;
            self.conns.lock().expect("соединения не отравлены").insert(conn, stream);
            if (200..400).contains(&status) { Ok(()) } else { Err(format!("статус {status}")) }
        })
    }
}
