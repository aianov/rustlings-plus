use std::io;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

async fn handle(stream: TcpStream) -> io::Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();
    while let Some(line) = lines.next_line().await? {
        if line == "boom" {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "клиент прислал boom"));
        }
        if line == "panic" {
            panic!("обработчик запаниковал");
        }
        writer.write_all(format!("{}\n", line.to_uppercase()).as_bytes()).await?;
    }
    Ok(())
}

async fn serve(listener: TcpListener) -> io::Result<()> {
    loop {
        let (stream, peer) = listener.accept().await?;
        tokio::spawn(async move {
            if let Err(e) = handle(stream).await {
                eprintln!("соединение {peer} закрыто с ошибкой: {e}");
            }
        });
    }
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    println!("слушаем {}", listener.local_addr()?);
    serve(listener).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;
    use std::time::Duration;
    use tokio::io::AsyncReadExt;
    use tokio::task::JoinHandle;
    use tokio::time::timeout;

    async fn start() -> (SocketAddr, JoinHandle<io::Result<()>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        (addr, tokio::spawn(serve(listener)))
    }

    async fn upper(addr: SocketAddr, word: &str) -> String {
        let mut s = TcpStream::connect(addr)
            .await
            .expect("сервер перестал принимать соединения: сбой одного клиента остановил serve");
        s.write_all(format!("{word}\n").as_bytes()).await.unwrap();
        let mut reply = String::new();
        timeout(Duration::from_secs(3), BufReader::new(s).read_line(&mut reply))
            .await
            .expect("сервер не ответил за 3 секунды")
            .unwrap();
        reply
    }

    async fn misbehave(addr: SocketAddr, word: &str) {
        let mut s = TcpStream::connect(addr).await.unwrap();
        s.write_all(format!("{word}\n").as_bytes()).await.unwrap();
        let mut rest = Vec::new();
        let _ = timeout(Duration::from_secs(3), s.read_to_end(&mut rest)).await;
    }

    #[tokio::test]
    async fn normal_client_works() {
        let (addr, _server) = start().await;
        assert_eq!(upper(addr, "hello").await, "HELLO\n", "обычная строка должна вернуться в верхнем регистре");
    }

    #[tokio::test]
    async fn handler_error_does_not_stop_the_server() {
        let (addr, server) = start().await;
        misbehave(addr, "boom").await;
        assert!(!server.is_finished(), "serve завершилась после ошибки в одном соединении: `?` на результате handle выносит ошибку из цикла");
        assert_eq!(upper(addr, "after boom").await, "AFTER BOOM\n", "после ошибки сервер должен обслуживать новых");
    }

    #[tokio::test]
    async fn handler_panic_does_not_stop_the_server() {
        let (addr, server) = start().await;
        misbehave(addr, "panic").await;
        assert!(!server.is_finished(), "паника обработчика остановила serve: обработчик должен жить в своей задаче");
        assert_eq!(upper(addr, "after panic").await, "AFTER PANIC\n", "после паники сервер должен обслуживать новых");
    }
}
