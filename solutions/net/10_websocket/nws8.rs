// Браузер шлёт рукопожатие и сразу за ним первое сообщение, и по быстрой сети оба оказываются в одном TCP-сегменте.
// Сервер отвечает 101, обработчик начинает читать сообщения... и первое сообщение у многих клиентов пропадает.
// У тех, кто подключается медленно, всё работает. Мини-протокол сообщений здесь простой: байт длины и текст.
// TODO: Найди, где теряются байты, и почини accept_upgrade и read_messages так, чтобы ничего не терялось.

#![allow(dead_code)]
use std::io;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

struct Upgraded<S> {
    stream: S,
    rest: Vec<u8>,
}

const SWITCH: &[u8] = b"HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\r\n";

fn find_head_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n").map(|p| p + 4)
}

async fn accept_upgrade<S>(mut stream: S) -> io::Result<Option<Upgraded<S>>>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let end = loop {
        if let Some(end) = find_head_end(&buf) {
            break end;
        }
        let n = stream.read(&mut chunk).await?;
        if n == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        buf.extend_from_slice(&chunk[..n]);
    };
    let head = String::from_utf8_lossy(&buf[..end]).into_owned();
    buf.drain(..end);
    if !head.starts_with("GET /ws ") {
        stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n").await?;
        return Ok(None);
    }
    stream.write_all(SWITCH).await?;
    Ok(Some(Upgraded { stream, rest: buf }))
}

async fn read_messages<S>(upgraded: Upgraded<S>, count: usize) -> io::Result<Vec<String>>
where
    S: AsyncRead + Unpin,
{
    let Upgraded { mut stream, rest } = upgraded;
    let mut buf = rest;
    let mut out = Vec::new();
    let mut chunk = [0u8; 64];
    while out.len() < count {
        if let Some(&len) = buf.first() {
            let len = usize::from(len);
            if buf.len() > len {
                out.push(String::from_utf8_lossy(&buf[1..=len]).into_owned());
                buf.drain(..=len);
                continue;
            }
        }
        let n = stream.read(&mut chunk).await?;
        if n == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    Ok(out)
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    use std::time::Duration;
    use tokio::io::duplex;
    use tokio::time::timeout;

    fn upgrade_request() -> Vec<u8> {
        b"GET /ws HTTP/1.1\r\nHost: x\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\r\n".to_vec()
    }

    fn message(text: &str) -> Vec<u8> {
        let mut out = vec![text.len() as u8];
        out.extend_from_slice(text.as_bytes());
        out
    }

    async fn run(first_write: Vec<u8>, later: Vec<Vec<u8>>, count: usize) -> Vec<String> {
        let (mut client, server) = duplex(64 * 1024);
        let task = tokio::spawn(async move {
            let upgraded = accept_upgrade(server).await?.expect("это /ws");
            read_messages(upgraded, count).await
        });
        client.write_all(&first_write).await.unwrap();
        for part in later {
            tokio::time::sleep(Duration::from_millis(20)).await;
            client.write_all(&part).await.unwrap();
        }
        let result = timeout(Duration::from_secs(3), task).await.expect("обработчик завис: ждёт сообщение, которое уже получил").unwrap();
        result.unwrap()
    }

    #[tokio::test]
    async fn message_in_the_same_segment_as_the_handshake() {
        let mut first = upgrade_request();
        first.extend(message("hello"));
        assert_eq!(run(first, vec![], 1).await, vec!["hello"], "сообщение пришло вместе с рукопожатием и не должно потеряться");
    }

    #[tokio::test]
    async fn several_messages_after_the_handshake_in_one_write() {
        let mut first = upgrade_request();
        first.extend(message("one"));
        first.extend(message("two"));
        first.extend(message("three"));
        assert_eq!(run(first, vec![], 3).await, vec!["one", "two", "three"]);
    }

    #[tokio::test]
    async fn a_message_cut_in_half_across_the_boundary() {
        let mut first = upgrade_request();
        first.extend(message("whole"));
        let long = message("split-in-two");
        first.extend_from_slice(&long[..5]);
        assert_eq!(run(first, vec![long[5..].to_vec()], 2).await, vec!["whole", "split-in-two"], "половина сообщения лежит в остатке, половина придёт позже");
    }

    #[tokio::test]
    async fn slow_client_still_works() {
        assert_eq!(run(upgrade_request(), vec![message("late")], 1).await, vec!["late"]);
    }

    #[tokio::test]
    async fn other_paths_get_404() {
        let (mut client, server) = duplex(4096);
        let task = tokio::spawn(async move { accept_upgrade(server).await.map(|u| u.is_none()) });
        client.write_all(b"GET /other HTTP/1.1\r\nHost: x\r\n\r\n").await.unwrap();
        let mut answer = vec![0u8; 12];
        timeout(Duration::from_secs(3), client.read_exact(&mut answer)).await.unwrap().unwrap();
        assert_eq!(&answer, b"HTTP/1.1 404");
        assert!(timeout(Duration::from_secs(3), task).await.unwrap().unwrap().unwrap());
    }
}
