// Мягкая остановка: перестать принимать новых, дать текущим закончить, но не ждать вечно.
// Резко убить процесс значит оборвать чьи-то запросы на полуслове, а ждать без дедлайна значит
// навсегда застрять на клиенте, который не уходит.
// TODO: Напиши с нуля serve_until: сервер принимает клиентов, пока не сработает shutdown. После
// этого новые клиенты получают отказ, а текущим даётся время grace на завершение. Что не успело,
// обрывается. Верни число оборванных соединений.

use std::future::Future;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinSet;
use tokio::time::timeout;

async fn serve_until<F, Fut>(listener: TcpListener, shutdown: impl Future<Output = ()>, grace: Duration, handler: F) -> usize
where
    F: Fn(TcpStream) -> Fut,
    Fut: Future<Output = ()> + Send + 'static,
{
    todo!()
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    println!("слушаем {}", listener.local_addr()?);
    let stop = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    let abandoned = serve_until(listener, stop, Duration::from_secs(5), |mut s: TcpStream| async move {
        let _ = s.write_all(b"hello\n").await;
    })
    .await;
    println!("брошено недожитых соединений: {abandoned}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;
    use std::time::Instant;
    use tokio::sync::oneshot;

    async fn start(grace: Duration) -> (SocketAddr, oneshot::Sender<()>, tokio::task::JoinHandle<usize>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = oneshot::channel::<()>();
        let task = tokio::spawn(serve_until(
            listener,
            async move {
                let _ = rx.await;
            },
            grace,
            |mut s: TcpStream| async move {
                let mut b = [0u8; 1];
                if s.read_exact(&mut b).await.is_ok() {
                    let _ = s.write_all(b"bye").await;
                }
            },
        ));
        (addr, tx, task)
    }

    #[tokio::test]
    async fn idle_server_stops_and_closes_the_port() {
        let (addr, stop, task) = start(Duration::from_secs(1)).await;
        stop.send(()).unwrap();
        let left = timeout(Duration::from_secs(3), task).await.expect("serve_until не вернулась после сигнала").unwrap();
        assert_eq!(left, 0, "соединений не было, а оборвано {left}");
        assert!(TcpStream::connect(addr).await.is_err(), "после остановки порт должен быть закрыт: слушателя надо отпустить");
    }

    #[tokio::test]
    async fn in_flight_connection_is_allowed_to_finish() {
        let (addr, stop, task) = start(Duration::from_secs(3)).await;
        let mut client = TcpStream::connect(addr).await.unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        stop.send(()).unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(!task.is_finished(), "соединение ещё живо, а serve_until уже вернулась: надо дождаться handler");
        client.write_all(b"x").await.unwrap();
        let mut reply = Vec::new();
        timeout(Duration::from_secs(3), client.read_to_end(&mut reply)).await.expect("завис").unwrap();
        assert_eq!(reply, b"bye", "текущее соединение должно было доработать после сигнала");
        let left = timeout(Duration::from_secs(3), task).await.expect("serve_until не вернулась").unwrap();
        assert_eq!(left, 0, "все успели закончиться, оборванных быть не должно");
    }

    #[tokio::test]
    async fn stubborn_connection_is_cut_after_grace() {
        let (addr, stop, task) = start(Duration::from_millis(200)).await;
        let _stubborn = TcpStream::connect(addr).await.unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        let started = Instant::now();
        stop.send(()).unwrap();
        let left = timeout(Duration::from_secs(3), task).await.expect("упрямый клиент завесил остановку: нужен дедлайн grace").unwrap();
        assert_eq!(left, 1, "один клиент не ушёл за grace, ждали 1 оборванное соединение");
        assert!(started.elapsed() >= Duration::from_millis(150), "остановились раньше дедлайна ({:?}): надо было дать время", started.elapsed());
    }

    #[tokio::test]
    async fn finished_connections_do_not_pile_up() {
        let (addr, stop, task) = start(Duration::from_secs(1)).await;
        for _ in 0..30 {
            let mut c = TcpStream::connect(addr).await.unwrap();
            c.write_all(b"x").await.unwrap();
            let mut r = Vec::new();
            timeout(Duration::from_secs(3), c.read_to_end(&mut r)).await.expect("завис").unwrap();
        }
        stop.send(()).unwrap();
        assert_eq!(timeout(Duration::from_secs(3), task).await.expect("завис").unwrap(), 0, "все 30 уже завершились");
    }
}
