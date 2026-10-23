// serve умеет остановиться: сигнал приходит через watch, accept-цикл выходит и ждёт соединения.
// Но стоит подключиться одному клиенту и промолчать, как остановка зависает навсегда: ждать-то
// мы ждём, а соединению о сигнале никто не сказал. Чини handle: при остановке он должен
// попрощаться строкой «bye» и закрыться, не дожидаясь клиента.

use std::io;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;
use tokio::task::JoinSet;

async fn handle(mut stream: TcpStream, _stop: watch::Receiver<bool>) -> io::Result<()> {
    let mut buf = [0u8; 1024];
    loop {
        let n = stream.read(&mut buf).await?;
        if n == 0 {
            return Ok(());
        }
        stream.write_all(&buf[..n]).await?;
    }
}

async fn serve(listener: TcpListener, mut stop: watch::Receiver<bool>) {
    let mut conns = JoinSet::new();
    loop {
        tokio::select! {
            _ = stop.changed() => break,
            Ok((stream, _)) = listener.accept() => {
                conns.spawn(handle(stream, stop.clone()));
            }
        }
    }
    while conns.join_next().await.is_some() {}
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    println!("слушаем {}", listener.local_addr()?);
    let (tx, rx) = watch::channel(false);
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        let _ = tx.send(true);
    });
    serve(listener, rx).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::timeout;

    async fn start() -> (std::net::SocketAddr, watch::Sender<bool>, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = watch::channel(false);
        (addr, tx, tokio::spawn(serve(listener, rx)))
    }

    #[tokio::test]
    async fn echo_still_works() {
        let (addr, _tx, _task) = start().await;
        let mut client = TcpStream::connect(addr).await.unwrap();
        client.write_all(b"hi").await.unwrap();
        let mut b = [0u8; 2];
        timeout(Duration::from_secs(3), client.read_exact(&mut b)).await.expect("завис").unwrap();
        assert_eq!(&b, b"hi", "эхо сломалось");
    }

    #[tokio::test]
    async fn stop_reaches_idle_connections() {
        let (addr, tx, task) = start().await;
        let mut client = TcpStream::connect(addr).await.unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        tx.send(true).unwrap();
        timeout(Duration::from_secs(3), task)
            .await
            .expect("сервер не остановился: соединение с молчащим клиентом не знает о сигнале и держит serve")
            .unwrap();
        let mut rest = Vec::new();
        timeout(Duration::from_secs(3), client.read_to_end(&mut rest)).await.expect("завис").unwrap();
        assert_eq!(rest, b"bye\n", "при остановке клиент должен получить bye и EOF");
    }

    #[tokio::test]
    async fn several_idle_clients_all_get_bye() {
        let (addr, tx, task) = start().await;
        let mut clients = Vec::new();
        for _ in 0..5 {
            clients.push(TcpStream::connect(addr).await.unwrap());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
        tx.send(true).unwrap();
        timeout(Duration::from_secs(3), task).await.expect("сервер не остановился").unwrap();
        for mut c in clients {
            let mut rest = Vec::new();
            timeout(Duration::from_secs(3), c.read_to_end(&mut rest)).await.expect("завис").unwrap();
            assert_eq!(rest, b"bye\n", "каждый клиент должен получить bye");
        }
    }
}
