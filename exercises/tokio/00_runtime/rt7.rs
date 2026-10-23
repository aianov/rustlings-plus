// Кто-то собрал рантаймы «по минимуму, чтобы быстрее стартовали». Теперь первый же
// sleep и первый же сокет падают паникой. Запусти тесты и прочитай сообщения: в них
// прямо сказано, что включить.
// TODO: Почини обе функции, не меняя тесты.
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};
use tokio::runtime::{Builder, Runtime};
use tokio::time::timeout;

fn timer_runtime() -> Runtime {
    Builder::new_current_thread().build().unwrap()
}

fn net_runtime() -> Runtime {
    Builder::new_current_thread().enable_time().build().unwrap()
}

fn main() {
    timer_runtime().block_on(async { tokio::time::sleep(Duration::from_millis(1)).await });
    println!("ok");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timers_work() {
        let value = timer_runtime().block_on(async {
            tokio::time::sleep(Duration::from_millis(5)).await;
            7
        });
        assert_eq!(value, 7, "sleep должен отработать");
    }

    #[test]
    fn sockets_work() {
        let port = net_runtime().block_on(async {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            listener.local_addr().unwrap().port()
        });
        assert_ne!(port, 0, "bind должен получить порт");
    }

    #[test]
    fn sockets_and_timers_together() {
        let connected = net_runtime().block_on(async {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            timeout(Duration::from_secs(2), TcpStream::connect(addr)).await.is_ok()
        });
        assert!(connected, "timeout вокруг connect требует и таймеров, и сети в одном рантайме");
    }
}
