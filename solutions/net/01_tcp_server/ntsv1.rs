use std::io;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

async fn echo(mut stream: TcpStream) -> io::Result<()> {
    let mut buf = [0u8; 1024];
    loop {
        let n = stream.read(&mut buf).await?;
        if n == 0 {
            return Ok(());
        }
        stream.write_all(&buf[..n]).await?;
    }
}

async fn serve(listener: TcpListener) -> io::Result<()> {
    loop {
        let (stream, _) = listener.accept().await?;
        tokio::spawn(async move {
            let _ = echo(stream).await;
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
    use tokio::time::timeout;

    async fn start() -> SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(serve(listener));
        addr
    }

    #[tokio::test]
    async fn second_client_is_not_blocked_by_the_first() {
        let addr = start().await;
        let _silent = TcpStream::connect(addr).await.unwrap();
        let mut second = TcpStream::connect(addr).await.unwrap();
        second.write_all(b"hi").await.unwrap();
        let mut buf = [0u8; 2];
        timeout(Duration::from_secs(3), second.read_exact(&mut buf))
            .await
            .expect("второй клиент не получил эхо, пока первый молчит: сервер обслуживает клиентов по очереди, а не параллельно")
            .unwrap();
        assert_eq!(&buf, b"hi", "эхо вернуло не то, что отправили");
    }

    #[tokio::test]
    async fn many_clients_at_once() {
        let addr = start().await;
        let mut tasks = Vec::new();
        for i in 0..20u8 {
            tasks.push(tokio::spawn(async move {
                let mut s = TcpStream::connect(addr).await.unwrap();
                s.write_all(&[i; 100]).await.unwrap();
                let mut buf = [0u8; 100];
                s.read_exact(&mut buf).await.unwrap();
                assert!(buf.iter().all(|b| *b == i), "клиент {i} получил чужие байты: данные разных соединений перемешались");
            }));
        }
        for t in tasks {
            timeout(Duration::from_secs(5), t).await.expect("клиенты не уложились в 5 секунд").unwrap();
        }
    }

    #[tokio::test]
    async fn server_keeps_running_after_clients_leave() {
        let addr = start().await;
        for _ in 0..3 {
            let mut s = TcpStream::connect(addr).await.unwrap();
            s.write_all(b"x").await.unwrap();
            let mut b = [0u8; 1];
            timeout(Duration::from_secs(3), s.read_exact(&mut b)).await.expect("завис").unwrap();
        }
    }
}
