use std::io;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};

async fn handle<S: AsyncRead + AsyncWrite + Unpin>(stream: S) -> io::Result<()> {
    let (reader, mut writer) = tokio::io::split(stream);
    let mut lines = BufReader::new(reader).lines();
    while let Some(line) = lines.next_line().await? {
        let reply = match line.as_str() {
            "PING" => "PONG".to_owned(),
            "QUIT" => {
                writer.write_all(b"BYE\n").await?;
                return Ok(());
            }
            other => match other.strip_prefix("ECHO ") {
                Some(text) => text.to_owned(),
                None => "ERR unknown".to_owned(),
            },
        };
        writer.write_all(format!("{reply}\n").as_bytes()).await?;
    }
    Ok(())
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    println!("слушаем {}", listener.local_addr()?);
    let (stream, _) = listener.accept().await?;
    handle(stream).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, duplex};
    use tokio::net::{TcpListener, TcpStream};
    use tokio::time::timeout;

    #[tokio::test]
    async fn answers_commands_over_duplex() {
        let (mut client, server) = duplex(1024);
        let task = tokio::spawn(handle(server));
        client.write_all(b"PING\nECHO hello world\nFOO\nQUIT\n").await.unwrap();
        let mut out = String::new();
        timeout(Duration::from_secs(3), client.read_to_string(&mut out))
            .await
            .expect("обработчик не закрыл поток после QUIT")
            .unwrap();
        assert_eq!(out, "PONG\nhello world\nERR unknown\nBYE\n", "ответы сервера не совпали с протоколом");
        assert!(task.await.unwrap().is_ok(), "после QUIT обработчик должен вернуть Ok");
    }

    #[tokio::test]
    async fn eof_ends_the_handler_quietly() {
        let (mut client, server) = duplex(1024);
        let task = tokio::spawn(handle(server));
        client.write_all(b"PING\n").await.unwrap();
        client.shutdown().await.unwrap();
        let mut out = String::new();
        timeout(Duration::from_secs(3), client.read_to_string(&mut out)).await.expect("завис на EOF").unwrap();
        assert_eq!(out, "PONG\n", "ответ на единственную команду");
        let res = timeout(Duration::from_secs(3), task).await.expect("handle не вернулся после EOF").unwrap();
        assert!(res.is_ok(), "конец потока это не ошибка");
    }

    #[tokio::test]
    async fn command_split_across_writes() {
        let (mut client, server) = duplex(1024);
        tokio::spawn(handle(server));
        client.write_all(b"PI").await.unwrap();
        tokio::time::sleep(Duration::from_millis(30)).await;
        client.write_all(b"NG\n").await.unwrap();
        let mut buf = [0u8; 5];
        timeout(Duration::from_secs(3), client.read_exact(&mut buf)).await.expect("завис").unwrap();
        assert_eq!(&buf, b"PONG\n", "команда пришла двумя кусками, а строка одна");
    }

    #[tokio::test]
    async fn same_handler_works_over_tcp() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            handle(stream).await
        });
        let mut client = TcpStream::connect(addr).await.unwrap();
        client.write_all(b"ECHO tcp\nQUIT\n").await.unwrap();
        let mut out = String::new();
        timeout(Duration::from_secs(3), client.read_to_string(&mut out)).await.expect("завис").unwrap();
        assert_eq!(out, "tcp\nBYE\n", "по TCP ответы те же, что и на duplex");
    }
}
