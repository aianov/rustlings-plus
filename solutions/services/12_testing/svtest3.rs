use std::net::SocketAddr;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;

fn main() {}

#[derive(Clone)]
pub struct Config {
    pub init_delay: Duration,
    pub fail_init: bool,
}

#[derive(Debug, PartialEq)]
pub enum StartError {
    Init(String),
}

pub struct Running {
    pub addr: SocketAddr,
    pub task: JoinHandle<()>,
}

async fn init(config: &Config) -> Result<(), String> {
    tokio::time::sleep(config.init_delay).await;
    if config.fail_init { Err("брокер недоступен".to_string()) } else { Ok(()) }
}

async fn serve(listener: TcpListener) {
    loop {
        let Ok((stream, _)) = listener.accept().await else { return };
        tokio::spawn(async move {
            let (read, mut write) = stream.into_split();
            let mut lines = BufReader::new(read).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let reply = if line == "PING" { "PONG\n" } else { "ERR\n" };
                if write.write_all(reply.as_bytes()).await.is_err() {
                    return;
                }
            }
        });
    }
}

pub async fn start(config: Config) -> Result<Running, StartError> {
    init(&config).await.map_err(StartError::Init)?;
    let listener = TcpListener::bind("127.0.0.1:0").await.map_err(|e| StartError::Init(e.to_string()))?;
    let addr = listener.local_addr().map_err(|e| StartError::Init(e.to_string()))?;
    let task = tokio::spawn(serve(listener));
    Ok(Running { addr, task })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{Instant, timeout};

    async fn ping(addr: SocketAddr) -> std::io::Result<String> {
        let mut stream = TcpStream::connect(addr).await?;
        stream.write_all(b"PING\n").await?;
        let mut line = String::new();
        BufReader::new(stream).read_line(&mut line).await?;
        Ok(line.trim().to_string())
    }

    #[tokio::test(start_paused = true)]
    async fn the_first_request_right_after_start_is_answered_at_once() {
        let began = Instant::now();
        let running = start(Config { init_delay: Duration::from_millis(500), fail_init: false }).await.expect("сервис запустился");
        let started = began.elapsed();
        assert!(started >= Duration::from_millis(500), "start вернулся через {started:?}: он обязан дождаться конца подготовки, а не обещать готовность заранее");
        let before = Instant::now();
        let answer = ping(running.addr).await.expect("порт уже слушает: отказа в соединении быть не должно");
        assert_eq!(answer, "PONG", "сервис отвечает");
        assert!(before.elapsed() < Duration::from_millis(50), "запрос ждал {:?}: сервис должен отвечать сразу, а не доделывать подготовку", before.elapsed());
        running.task.abort();
    }

    #[tokio::test(start_paused = true)]
    async fn a_failed_init_is_an_error_not_a_hang() {
        let result = timeout(Duration::from_secs(5), start(Config { init_delay: Duration::from_millis(100), fail_init: true })).await.expect("start не должен висеть, когда подготовка упала");
        assert!(matches!(result, Err(StartError::Init(ref why)) if why.contains("брокер")), "ошибка подготовки доходит с причиной");
    }

    #[tokio::test(start_paused = true)]
    async fn nothing_listens_before_the_init_is_done() {
        let config = Config { init_delay: Duration::from_millis(300), fail_init: false };
        let starting = tokio::spawn(start(config));
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(!starting.is_finished(), "на 100-й миллисекунде подготовка ещё идёт, и start не должен был вернуться");
        let running = starting.await.unwrap().expect("запустился");
        assert!(ping(running.addr).await.is_ok(), "после возврата сервис отвечает");
        running.task.abort();
    }

    #[tokio::test(start_paused = true)]
    async fn two_services_in_one_test_do_not_collide() {
        let config = Config { init_delay: Duration::from_millis(10), fail_init: false };
        let (a, b) = tokio::join!(start(config.clone()), start(config));
        let (a, b) = (a.unwrap(), b.unwrap());
        assert_ne!(a.addr, b.addr, "у каждого свой порт");
        assert_eq!(ping(a.addr).await.unwrap(), "PONG", "первый отвечает");
        assert_eq!(ping(b.addr).await.unwrap(), "PONG", "второй отвечает");
        a.task.abort();
        b.task.abort();
    }
}
