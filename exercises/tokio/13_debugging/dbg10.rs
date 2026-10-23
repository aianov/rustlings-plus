// Хаб принимает подключения и отвечает эхом. Сервис работает, клиенты довольны, но к вечеру число
// живых задач в рантайме выросло в разы, а клиентов осталось столько же. Каждое подключение оставляет
// что-то за собой после отключения. Найди, что именно, и почини так, чтобы после ухода клиента не
// оставалось ни задач, ни записей о нём.
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream, WriteHalf};
use tokio::sync::mpsc;

type Conns = Arc<Mutex<HashMap<u32, mpsc::Sender<String>>>>;

struct Hub {
    conns: Conns,
    next: AtomicU32,
}

async fn writer(mut write: WriteHalf<DuplexStream>, mut out: mpsc::Receiver<String>) {
    while let Some(text) = out.recv().await {
        if write.write_all(format!("{text}\n").as_bytes()).await.is_err() {
            break;
        }
    }
}

impl Hub {
    fn new() -> Hub {
        Hub { conns: Arc::new(Mutex::new(HashMap::new())), next: AtomicU32::new(1) }
    }

    fn connections(&self) -> usize {
        self.conns.lock().unwrap().len()
    }

    async fn say_all(&self, text: &str) {
        let senders: Vec<_> = self.conns.lock().unwrap().values().cloned().collect();
        for sender in senders {
            let _ = sender.send(text.to_string()).await;
        }
    }

    fn accept(&self, stream: DuplexStream) {
        let id = self.next.fetch_add(1, Ordering::SeqCst);
        let (out_tx, out_rx) = mpsc::channel(8);
        self.conns.lock().unwrap().insert(id, out_tx.clone());
        let (read, write) = tokio::io::split(stream);
        tokio::spawn(writer(write, out_rx));
        tokio::spawn(async move {
            let mut lines = BufReader::new(read).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if out_tx.send(format!("echo {line}")).await.is_err() {
                    break;
                }
            }
        });
    }
}

#[tokio::main]
async fn main() {
    let hub = Hub::new();
    let (_client, server) = tokio::io::duplex(256);
    hub.accept(server);
    println!("{}", hub.connections());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::runtime::Handle;
    use tokio::time::{sleep, timeout};

    fn connect(hub: &Hub) -> BufReader<DuplexStream> {
        let (client, server) = tokio::io::duplex(256);
        hub.accept(server);
        BufReader::new(client)
    }

    async fn ask(client: &mut BufReader<DuplexStream>, text: &str) -> String {
        client.get_mut().write_all(format!("{text}\n").as_bytes()).await.unwrap();
        let mut line = String::new();
        timeout(Duration::from_secs(1), client.read_line(&mut line)).await.expect("ответа нет").unwrap();
        line.trim_end().to_string()
    }

    #[tokio::test(start_paused = true)]
    async fn the_hub_echoes_and_broadcasts() {
        let hub = Hub::new();
        let mut a = connect(&hub);
        let mut b = connect(&hub);
        assert_eq!(ask(&mut a, "привет").await, "echo привет", "эхо должно работать");
        hub.say_all("всем").await;
        let mut line = String::new();
        timeout(Duration::from_secs(1), b.read_line(&mut line)).await.expect("рассылка не дошла").unwrap();
        assert_eq!(line.trim_end(), "всем", "рассылка доходит до подключённых");
        assert_eq!(hub.connections(), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn nothing_is_left_after_clients_leave() {
        let metrics = Handle::current().metrics();
        let baseline = metrics.num_alive_tasks();
        let hub = Hub::new();
        let mut clients = Vec::new();
        for i in 0..10 {
            let mut client = connect(&hub);
            assert_eq!(ask(&mut client, &format!("m{i}")).await, format!("echo m{i}"));
            clients.push(client);
        }
        assert_eq!(hub.connections(), 10, "десять клиентов подключены");
        drop(clients);
        sleep(Duration::from_millis(500)).await;
        assert_eq!(hub.connections(), 0, "после ухода клиентов хаб не должен помнить их подключения");
        let left = metrics.num_alive_tasks() - baseline;
        assert_eq!(left, 0, "после ухода всех клиентов в рантайме осталось лишних задач: {left}");
    }

    #[tokio::test(start_paused = true)]
    async fn one_client_leaving_does_not_disturb_the_others() {
        let hub = Hub::new();
        let mut a = connect(&hub);
        let b = connect(&hub);
        assert_eq!(ask(&mut a, "x").await, "echo x");
        drop(b);
        sleep(Duration::from_millis(100)).await;
        assert_eq!(hub.connections(), 1, "остался один клиент");
        assert_eq!(ask(&mut a, "y").await, "echo y", "оставшийся клиент работает как раньше");
    }
}
