use std::io;
use std::net::SocketAddr;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

async fn fetch_raw(addr: SocketAddr, host: &str, path: &str) -> io::Result<String> {
    let mut stream = TcpStream::connect(addr).await?;
    let request = format!("GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).await?;
    let mut response = String::new();
    stream.read_to_string(&mut response).await?;
    Ok(response)
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let addr: SocketAddr = "127.0.0.1:8080".parse().expect("адрес");
    println!("{}", fetch_raw(addr, "localhost", "/").await?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::net::TcpListener;
    use tokio::sync::oneshot;
    use tokio::time::{sleep, timeout};

    async fn spawn_server() -> (SocketAddr, oneshot::Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = oneshot::channel();
        tokio::spawn(async move {
            let (mut s, _) = listener.accept().await.unwrap();
            let mut got = Vec::new();
            let mut buf = [0u8; 512];
            while !got.windows(4).any(|w| w == b"\r\n\r\n") {
                let n = s.read(&mut buf).await.unwrap_or(0);
                if n == 0 {
                    return;
                }
                got.extend_from_slice(&buf[..n]);
            }
            let text = String::from_utf8_lossy(&got).to_string();
            let close = text.to_ascii_lowercase().contains("\r\nconnection: close\r\n");
            let _ = tx.send(text);
            let _ = s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: close\r\n\r\nhello").await;
            if !close {
                sleep(Duration::from_secs(30)).await;
            }
        });
        (addr, rx)
    }

    async fn run(path: &str) -> (String, String) {
        let (addr, rx) = spawn_server().await;
        let response = timeout(Duration::from_secs(3), fetch_raw(addr, "example.test", path))
            .await
            .expect("fetch_raw завис: сервер держит соединение открытым, пока клиент не скажет, что после ответа его можно закрыть, а без этого read_to_string ждёт конца вечно")
            .expect("fetch_raw вернул ошибку");
        let request = timeout(Duration::from_secs(1), rx).await.expect("сервер не дождался запроса").expect("сервер не получил полного запроса: запрос заканчивается пустой строкой");
        (request, response)
    }

    #[tokio::test]
    async fn returns_the_whole_response_text() {
        let (_, response) = run("/ping").await;
        assert!(response.starts_with("HTTP/1.1 200 OK\r\n"), "ответ должен начинаться со строки статуса: {response:?}");
        assert!(response.ends_with("\r\n\r\nhello"), "ответ должен включать пустую строку и тело: {response:?}");
    }

    #[tokio::test]
    async fn request_has_the_right_shape() {
        let (request, _) = run("/ping").await;
        assert!(request.starts_with("GET /ping HTTP/1.1\r\n"), "первая строка запроса: метод, путь и версия через пробел, затем CRLF: {request:?}");
        assert!(request.contains("\r\nHost: example.test\r\n"), "в HTTP/1.1 заголовок Host обязателен: {request:?}");
        assert!(request.ends_with("\r\n\r\n"), "запрос заканчивается пустой строкой: {request:?}");
        let bytes = request.as_bytes();
        for (i, b) in bytes.iter().enumerate() {
            assert!(*b != b'\n' || (i > 0 && bytes[i - 1] == b'\r'), "строки HTTP разделяются парой CR LF, а не одним LF: {request:?}");
        }
    }
}
