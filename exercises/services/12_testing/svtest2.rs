// Тесту сервиса нужен настоящий сервис: поднять его в том же процессе, подключиться по сети и убрать за собой. Порт
// зашивать нельзя: параллельные тесты подерутся за него. Цель: `TestServer::start` занимает свободный порт (порт 0 просит
// систему выбрать сам), к моменту возврата уже слушает, так что клиент может подключаться сразу. `addr()` говорит адрес.
// `stop` сообщает всем остановиться и дожидается, пока закончатся и приём соединений, и все обработчики, в том числе
// у молчащих клиентов: после него в процессе не остаётся висящих задач, а порт закрыт.

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;
use tokio::task::{JoinHandle, JoinSet};

fn main() {}

struct Live(Arc<AtomicUsize>);

impl Live {
    fn enter(counter: &Arc<AtomicUsize>) -> Live {
        counter.fetch_add(1, Ordering::SeqCst);
        Live(counter.clone())
    }
}

impl Drop for Live {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

async fn handle(stream: TcpStream, mut shutdown: watch::Receiver<bool>, live: Arc<AtomicUsize>) {
    let _live = Live::enter(&live);
    let (read, mut write) = stream.into_split();
    let mut lines = BufReader::new(read).lines();
    loop {
        tokio::select! {
            _ = shutdown.changed() => return,
            line = lines.next_line() => {
                let Ok(Some(line)) = line else { return };
                let reply = match line.split_once(' ') {
                    Some(("ECHO", rest)) => rest.to_string(),
                    _ if line == "PING" => "PONG".to_string(),
                    _ => "ERR".to_string(),
                };
                if write.write_all(format!("{reply}\n").as_bytes()).await.is_err() {
                    return;
                }
            }
        }
    }
}

pub struct TestServer {
    addr: SocketAddr,
    shutdown: watch::Sender<bool>,
    accept: JoinHandle<()>,
    live: Arc<AtomicUsize>,
}

impl TestServer {
    pub async fn start() -> TestServer {
        let _ = (handle, JoinSet::<()>::new(), TcpListener::bind("127.0.0.1:0"), watch::channel(false));
        todo!()
    }

    pub async fn stop(self) {
        let _ = (self.shutdown, self.accept, self.live);
        todo!()
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    pub fn live_connections(&self) -> usize {
        self.live.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::io::AsyncReadExt;
    use tokio::time::timeout;

    async fn ask(stream: &mut TcpStream, line: &str) -> String {
        stream.write_all(format!("{line}\n").as_bytes()).await.unwrap();
        let mut reply = Vec::new();
        loop {
            let byte = timeout(Duration::from_secs(2), stream.read_u8()).await.expect("ответ за две секунды").expect("чтение");
            if byte == b'\n' {
                return String::from_utf8(reply).unwrap();
            }
            reply.push(byte);
        }
    }

    #[tokio::test]
    async fn a_client_can_connect_the_moment_start_returns() {
        let server = timeout(Duration::from_secs(5), TestServer::start()).await.expect("start не завис");
        let mut client = TcpStream::connect(server.addr()).await.expect("сервер уже слушает, когда start вернулся");
        assert_eq!(ask(&mut client, "PING").await, "PONG", "сервис отвечает");
        assert_eq!(ask(&mut client, "ECHO привет").await, "привет", "и эхо работает");
        drop(client);
        timeout(Duration::from_secs(5), server.stop()).await.expect("stop не завис");
    }

    #[tokio::test]
    async fn two_servers_get_two_different_ports() {
        let (a, b) = (TestServer::start().await, TestServer::start().await);
        assert_ne!(a.addr().port(), b.addr().port(), "каждый тест получает свой порт: фиксированный порт сделал бы параллельные тесты хрупкими");
        assert_ne!(a.addr().port(), 0, "адрес настоящий, а не нулевой порт");
        a.stop().await;
        b.stop().await;
    }

    #[tokio::test]
    async fn stop_ends_idle_connections_and_leaves_no_tasks() {
        let server = TestServer::start().await;
        let addr = server.addr();
        let mut idle = TcpStream::connect(addr).await.unwrap();
        let mut busy = TcpStream::connect(addr).await.unwrap();
        assert_eq!(ask(&mut busy, "PING").await, "PONG", "оба клиента на связи");
        assert_eq!(ask(&mut idle, "PING").await, "PONG", "и второй тоже");
        assert_eq!(server.live_connections(), 2, "два обработчика работают");
        let live = server.live.clone();
        timeout(Duration::from_secs(5), server.stop()).await.expect("stop должен закончиться, даже когда клиенты молчат и не закрывают соединение");
        assert_eq!(live.load(Ordering::SeqCst), 0, "после stop не осталось ни одного обработчика: он дождался их всех");
        let mut byte = [0u8; 1];
        let read = timeout(Duration::from_secs(2), idle.read(&mut byte)).await.expect("клиент увидел закрытие").expect("чтение после закрытия");
        assert_eq!(read, 0, "молчавший клиент получил конец потока");
    }

    #[tokio::test]
    async fn after_stop_the_port_is_closed() {
        let server = TestServer::start().await;
        let addr = server.addr();
        server.stop().await;
        assert!(TcpStream::connect(addr).await.is_err(), "порт после stop закрыт: слушающий сокет закрыт к моменту, когда stop вернулся");
    }

    #[tokio::test]
    async fn a_server_with_no_clients_stops_at_once() {
        let server = TestServer::start().await;
        timeout(Duration::from_millis(500), server.stop()).await.expect("остановка без клиентов не должна ждать ничего");
    }
}
