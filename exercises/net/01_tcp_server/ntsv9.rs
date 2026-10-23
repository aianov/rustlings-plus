// Клиент, который подключился и молчит, занимает дескриптор, память и место под лимитом
// соединений. Такое соединение пора закрывать. Простой считается от последнего принятого байта:
// пока клиент шлёт хотя бы по байту, он жив.
// TODO: Допиши echo_with_idle: эхо, где каждое чтение ограничено временем idle. Когда клиент
// закрыл запись, верни Closed. Когда прошло idle без единого байта, верни Idle.

use std::io;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;

#[derive(Debug, PartialEq)]
enum End {
    Closed,
    Idle,
}

async fn echo_with_idle(stream: &mut TcpStream, idle: Duration) -> io::Result<End> {
    todo!()
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let mut stream = TcpStream::connect("127.0.0.1:7").await?;
    println!("{:?}", echo_with_idle(&mut stream, Duration::from_secs(30)).await?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;
    use tokio::net::TcpListener;
    use tokio::time::sleep;

    async fn serve_once(idle: Duration) -> (TcpStream, tokio::task::JoinHandle<io::Result<End>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            echo_with_idle(&mut stream, idle).await
        });
        (TcpStream::connect(addr).await.unwrap(), server)
    }

    #[tokio::test]
    async fn silent_client_is_idle() {
        let (_client, server) = serve_once(Duration::from_millis(150)).await;
        let started = Instant::now();
        let end = timeout(Duration::from_secs(3), server).await.expect("сервер не закрыл молчащего клиента").unwrap().unwrap();
        assert_eq!(end, End::Idle, "клиент молчал дольше лимита, ждали Idle");
        assert!(started.elapsed() >= Duration::from_millis(120), "Idle вернулся слишком рано ({:?})", started.elapsed());
    }

    #[tokio::test]
    async fn talking_client_is_not_idle() {
        let (mut client, server) = serve_once(Duration::from_millis(150)).await;
        for _ in 0..5 {
            client.write_all(b"x").await.unwrap();
            sleep(Duration::from_millis(60)).await;
        }
        client.shutdown().await.unwrap();
        let end = timeout(Duration::from_secs(3), server).await.expect("завис").unwrap().unwrap();
        assert_eq!(end, End::Closed, "клиент писал каждые 60 мс при лимите 150: простой считается от последнего байта, а не от начала");
        let mut echoed = Vec::new();
        client.read_to_end(&mut echoed).await.unwrap();
        assert_eq!(echoed, b"xxxxx", "эхо должно вернуть все байты");
    }

    #[tokio::test]
    async fn closed_client_is_closed() {
        let (mut client, server) = serve_once(Duration::from_secs(5)).await;
        client.shutdown().await.unwrap();
        let end = timeout(Duration::from_secs(3), server).await.expect("завис на EOF").unwrap().unwrap();
        assert_eq!(end, End::Closed, "EOF это Closed, а не Idle");
    }
}
