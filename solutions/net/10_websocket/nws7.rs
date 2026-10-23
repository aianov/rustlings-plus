// Движок из прошлых глав живёт по правилу «запрос, ответ, ждём следующий запрос». После `101 Switching Protocols`
// это правило кончается: по сокету идут уже не HTTP-запросы, а чужой протокол, и разбирать их как запросы нельзя.
// Обработчик должен получить сокет в своё владение, а цикл соединения не должен больше им пользоваться.
// TODO: Допиши ветку Upgrade в serve_connection: клиент получает ответ 101 (он уже собран), а дальше сокетом владеет
// takeover. Подумай, когда соединение считается законченным.

#![allow(dead_code)]
use std::future::Future;
use std::io;
use std::pin::Pin;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

type Takeover<S> = Box<dyn FnOnce(S) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send>;

enum Reply<S> {
    Respond(Vec<u8>),
    Upgrade { response: Vec<u8>, takeover: Takeover<S> },
}

fn find_head_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n").map(|p| p + 4)
}

async fn serve_connection<S, A>(mut stream: S, app: A) -> io::Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    A: Fn(&str) -> Reply<S>,
{
    let mut buf = Vec::new();
    let mut chunk = [0u8; 1024];
    loop {
        let end = loop {
            if let Some(end) = find_head_end(&buf) {
                break end;
            }
            let n = stream.read(&mut chunk).await?;
            if n == 0 {
                return Ok(());
            }
            buf.extend_from_slice(&chunk[..n]);
        };
        let head = String::from_utf8_lossy(&buf[..end]).into_owned();
        buf.drain(..end);
        match app(&head) {
            Reply::Respond(bytes) => {
                stream.write_all(&bytes).await?;
            }
            Reply::Upgrade { response, takeover } => {
                stream.write_all(&response).await?;
                stream.flush().await?;
                takeover(stream).await;
                return Ok(());
            }
        }
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    use std::time::Duration;
    use tokio::io::{DuplexStream, duplex};
    use tokio::sync::oneshot;
    use tokio::time::timeout;

    const SWITCH: &[u8] = b"HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\r\n";
    const OK: &[u8] = b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok";

    fn echo_app(head: &str) -> Reply<DuplexStream> {
        if head.starts_with("GET /ws ") {
            Reply::Upgrade {
                response: SWITCH.to_vec(),
                takeover: Box::new(|mut stream: DuplexStream| {
                    Box::pin(async move {
                        let mut buf = [0u8; 256];
                        loop {
                            match stream.read(&mut buf).await {
                                Ok(0) | Err(_) => break,
                                Ok(n) => {
                                    if stream.write_all(&buf[..n]).await.is_err() {
                                        break;
                                    }
                                }
                            }
                        }
                    })
                }),
            }
        } else {
            Reply::Respond(OK.to_vec())
        }
    }

    async fn read_exactly(client: &mut DuplexStream, n: usize, what: &str) -> Vec<u8> {
        let mut out = vec![0u8; n];
        match timeout(Duration::from_secs(3), client.read_exact(&mut out)).await {
            Ok(Ok(_)) => out,
            Ok(Err(e)) => panic!("{what}: соединение оборвалось ({e})"),
            Err(_) => panic!("{what}: за 3 секунды ничего не пришло"),
        }
    }

    #[tokio::test]
    async fn after_101_bytes_belong_to_the_handler() {
        let (mut client, server) = duplex(64 * 1024);
        let task = tokio::spawn(serve_connection(server, echo_app));
        client.write_all(b"GET /ws HTTP/1.1\r\nHost: x\r\n\r\n").await.unwrap();
        assert_eq!(read_exactly(&mut client, SWITCH.len(), "ответ 101").await, SWITCH);
        let payload = b"GET / HTTP/1.1\r\nHost: x\r\n\r\n";
        client.write_all(payload).await.unwrap();
        assert_eq!(
            read_exactly(&mut client, payload.len(), "эхо").await,
            payload,
            "после 101 цикл соединения не имеет права разбирать байты как HTTP-запрос: они принадлежат обработчику"
        );
        drop(client);
        let done = timeout(Duration::from_secs(3), task).await.expect("соединение должно закончиться после обработчика");
        assert!(done.unwrap().is_ok());
    }

    #[tokio::test]
    async fn plain_requests_before_the_upgrade_still_work() {
        let (mut client, server) = duplex(64 * 1024);
        let task = tokio::spawn(serve_connection(server, echo_app));
        for _ in 0..2 {
            client.write_all(b"GET / HTTP/1.1\r\nHost: x\r\n\r\n").await.unwrap();
            assert_eq!(read_exactly(&mut client, OK.len(), "обычный ответ").await, OK);
        }
        client.write_all(b"GET /ws HTTP/1.1\r\nHost: x\r\n\r\n").await.unwrap();
        assert_eq!(read_exactly(&mut client, SWITCH.len(), "ответ 101").await, SWITCH);
        client.write_all(b"ping").await.unwrap();
        assert_eq!(read_exactly(&mut client, 4, "эхо").await, b"ping");
        drop(client);
        let _ = timeout(Duration::from_secs(3), task).await.expect("конец");
    }

    #[tokio::test]
    async fn the_response_is_sent_before_the_handler_runs() {
        let (mut client, server) = duplex(64 * 1024);
        let (release_tx, release_rx) = oneshot::channel::<()>();
        let release_rx = std::sync::Mutex::new(Some(release_rx));
        let app = move |head: &str| -> Reply<DuplexStream> {
            let _ = head;
            let rx = release_rx.lock().unwrap().take().expect("один апгрейд");
            Reply::Upgrade { response: SWITCH.to_vec(), takeover: Box::new(move |_s| Box::pin(async move { let _ = rx.await; })) }
        };
        let task = tokio::spawn(serve_connection(server, app));
        client.write_all(b"GET /ws HTTP/1.1\r\n\r\n").await.unwrap();
        assert_eq!(read_exactly(&mut client, SWITCH.len(), "101 приходит, даже если обработчик ещё ничего не читает").await, SWITCH);
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(!task.is_finished(), "соединение должно жить, пока жив обработчик: иначе сокет закроется у него под ногами и мягкая остановка не дождётся его");
        release_tx.send(()).unwrap();
        let done = timeout(Duration::from_secs(3), task).await.expect("после обработчика соединение заканчивается");
        assert!(done.unwrap().is_ok());
    }
}
