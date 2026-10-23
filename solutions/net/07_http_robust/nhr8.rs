use tokio::sync::mpsc;

use std::io;
#[allow(unused_imports)]
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

#[derive(Debug, Clone, PartialEq)]
struct Request {
    method: String,
    path: String,
    minor: u8,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

#[allow(dead_code)]
impl Request {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Response {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

#[allow(dead_code)]
impl Response {
    fn new(status: u16, body: &str) -> Response {
        Response { status, headers: Vec::new(), body: body.as_bytes().to_vec() }
    }

    fn header(mut self, name: &str, value: &str) -> Response {
        self.headers.push((name.to_string(), value.to_string()));
        self
    }
}

#[allow(dead_code)]
fn reason(status: u16) -> &'static str {
    match status {
        100 => "Continue",
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        401 => "Unauthorized",
        408 => "Request Timeout",
        413 => "Payload Too Large",
        417 => "Expectation Failed",
        431 => "Request Header Fields Too Large",
        500 => "Internal Server Error",
        _ => "",
    }
}

#[allow(dead_code)]
async fn write_response<W: AsyncWrite + Unpin>(w: &mut W, response: &Response) -> io::Result<()> {
    let mut out = format!("HTTP/1.1 {} {}\r\n", response.status, reason(response.status)).into_bytes();
    for (name, value) in &response.headers {
        out.extend_from_slice(format!("{name}: {value}\r\n").as_bytes());
    }
    out.extend_from_slice(format!("Content-Length: {}\r\n\r\n", response.body.len()).as_bytes());
    out.extend_from_slice(&response.body);
    w.write_all(&out).await
}

#[allow(dead_code)]
fn find_head_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n").map(|p| p + 4)
}

#[allow(dead_code)]
fn parse_head(head: &str) -> Option<Request> {
    let mut lines = head.split("\r\n");
    let mut parts = lines.next()?.split(' ');
    let (method, path, version) = (parts.next()?, parts.next()?, parts.next()?);
    let minor = match version {
        "HTTP/1.0" => 0,
        "HTTP/1.1" => 1,
        _ => return None,
    };
    if parts.next().is_some() || !path.starts_with('/') {
        return None;
    }
    let mut headers = Vec::new();
    for line in lines.filter(|l| !l.is_empty()) {
        let (name, value) = line.split_once(':')?;
        headers.push((name.trim().to_string(), value.trim().to_string()));
    }
    Some(Request { method: method.to_string(), path: path.to_string(), minor, headers, body: Vec::new() })
}

#[allow(dead_code)]
fn content_length(request: &Request) -> usize {
    request.header("content-length").and_then(|v| v.parse().ok()).unwrap_or(0)
}

async fn write_stream<W: AsyncWrite + Unpin>(
    w: &mut W,
    status: u16,
    content_type: &str,
    mut chunks: mpsc::Receiver<Vec<u8>>,
) -> io::Result<()> {
    let head = format!(
        "HTTP/1.1 {status} {}\r\nContent-Type: {content_type}\r\nTransfer-Encoding: chunked\r\n\r\n",
        reason(status)
    );
    w.write_all(head.as_bytes()).await?;
    w.flush().await?;
    while let Some(data) = chunks.recv().await {
        if data.is_empty() {
            continue;
        }
        let mut frame = format!("{:x}\r\n", data.len()).into_bytes();
        frame.extend_from_slice(&data);
        frame.extend_from_slice(b"\r\n");
        w.write_all(&frame).await?;
        w.flush().await?;
    }
    w.write_all(b"0\r\n\r\n").await?;
    w.flush().await
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let (tx, rx) = mpsc::channel(4);
    let (mut client, mut server) = tokio::io::duplex(4096);
    tokio::spawn(async move {
        for word in ["one ", "two ", "three"] {
            tx.send(word.as_bytes().to_vec()).await.unwrap();
        }
    });
    write_stream(&mut server, 200, "text/plain", rx).await?;
    drop(server);
    let mut out = String::new();
    client.read_to_string(&mut out).await?;
    println!("{out}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::io::{BufWriter, DuplexStream, duplex};
    use tokio::time::timeout;

    async fn line(client: &mut DuplexStream, buf: &mut Vec<u8>) -> String {
        loop {
            if let Some(at) = buf.windows(2).position(|w| w == b"\r\n") {
                let raw: Vec<u8> = buf.drain(..at + 2).collect();
                return String::from_utf8(raw[..at].to_vec()).unwrap();
            }
            let mut chunk = [0u8; 256];
            let n = timeout(Duration::from_secs(2), client.read(&mut chunk))
                .await
                .expect("кусок так и не пришёл: данные застряли у сервера, а клиент ждёт")
                .unwrap();
            assert!(n > 0, "соединение закрыто раньше времени");
            buf.extend_from_slice(&chunk[..n]);
        }
    }

    #[tokio::test]
    async fn framing_and_incremental_delivery() {
        let (mut client, server) = duplex(4096);
        let (tx, rx) = mpsc::channel(4);
        let writer = tokio::spawn(async move {
            let mut w = BufWriter::new(server);
            write_stream(&mut w, 200, "text/plain", rx).await
        });
        let mut buf = Vec::new();
        assert_eq!(line(&mut client, &mut buf).await, "HTTP/1.1 200 OK");
        let mut head = Vec::new();
        loop {
            let l = line(&mut client, &mut buf).await;
            if l.is_empty() {
                break;
            }
            head.push(l.to_ascii_lowercase());
        }
        assert!(head.contains(&"transfer-encoding: chunked".to_string()), "{head:?}");
        assert!(head.contains(&"content-type: text/plain".to_string()), "{head:?}");
        assert!(!head.iter().any(|h| h.starts_with("content-length")), "длины нет: {head:?}");

        tx.send(b"hello".to_vec()).await.unwrap();
        assert_eq!(line(&mut client, &mut buf).await, "5", "первый кусок должен дойти до клиента, пока поток не закончился (после каждого куска нужен flush)");
        assert_eq!(line(&mut client, &mut buf).await, "hello");
        tx.send(Vec::new()).await.unwrap();
        tx.send(vec![b'x'; 26]).await.unwrap();
        assert_eq!(line(&mut client, &mut buf).await, "1a", "размер в hex, пустой кусок пропущен");
        assert_eq!(line(&mut client, &mut buf).await, "x".repeat(26));
        drop(tx);
        assert_eq!(line(&mut client, &mut buf).await, "0");
        assert_eq!(line(&mut client, &mut buf).await, "");
        timeout(Duration::from_secs(2), writer).await.expect("write_stream не завершился после закрытия канала").unwrap().unwrap();
    }

    #[tokio::test]
    async fn empty_stream_is_still_a_valid_body() {
        let (mut client, mut server) = duplex(4096);
        let (tx, rx) = mpsc::channel(1);
        drop(tx);
        write_stream(&mut server, 200, "text/plain", rx).await.unwrap();
        drop(server);
        let mut out = String::new();
        client.read_to_string(&mut out).await.unwrap();
        assert!(out.starts_with("HTTP/1.1 200 OK\r\n"), "{out:?}");
        assert!(out.ends_with("\r\n\r\n0\r\n\r\n"), "пустой поток это только нулевой кусок: {out:?}");
    }

    #[tokio::test]
    async fn client_gone_is_an_error_not_a_hang() {
        let (client, mut server) = duplex(16);
        drop(client);
        let (tx, rx) = mpsc::channel(4);
        tx.send(vec![b'a'; 100]).await.unwrap();
        let result = timeout(Duration::from_secs(2), write_stream(&mut server, 200, "text/plain", rx)).await.expect("завис на закрытом соединении");
        assert!(result.is_err(), "клиент ушёл: ошибку записи нужно вернуть, а не глотать");
    }
}
