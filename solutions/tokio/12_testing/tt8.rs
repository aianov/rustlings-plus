use std::time::Duration;
use tokio::runtime::Handle;
use tokio::sync::mpsc;
use tokio::time::sleep;

struct Conn {
    tx: mpsc::Sender<String>,
}

impl Conn {
    async fn say(&self, text: &str) {
        let _ = self.tx.send(text.to_string()).await;
    }
}

trait Hub {
    fn connect() -> Conn;
}

struct Good;
struct Leaky;

async fn pump(mut rx: mpsc::Receiver<String>, tail: Duration) {
    while let Some(_message) = rx.recv().await {
        sleep(Duration::from_millis(1)).await;
    }
    sleep(tail).await;
}

impl Hub for Good {
    fn connect() -> Conn {
        let (tx, rx) = mpsc::channel(4);
        tokio::spawn(pump(rx, Duration::from_millis(20)));
        Conn { tx }
    }
}

impl Hub for Leaky {
    fn connect() -> Conn {
        let (tx, rx) = mpsc::channel(4);
        let own = tx.clone();
        tokio::spawn(async move {
            let _own = own;
            pump(rx, Duration::from_millis(20)).await;
        });
        Conn { tx }
    }
}


#[derive(Debug, PartialEq)]
enum Verdict {
    Passed,
    Failed,
    Hung,
}

fn verdict<F: FnOnce() + Send + 'static>(secs: u64, run: F) -> Verdict {
    let (done, finished) = std::sync::mpsc::channel();
    let worker = std::thread::Builder::new()
        .spawn(move || {
            run();
            let _ = done.send(());
        })
        .unwrap();
    match finished.recv_timeout(Duration::from_secs(secs)) {
        Ok(()) => {
            worker.join().unwrap();
            Verdict::Passed
        }
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => Verdict::Failed,
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Verdict::Hung,
    }
}

fn suite<H: Hub>() {
    let rt = tokio::runtime::Builder::new_current_thread().enable_time().start_paused(true).build().unwrap();
    rt.block_on(async {
        let metrics = Handle::current().metrics();
        let baseline = metrics.num_alive_tasks();
        let conns: Vec<Conn> = (0..10).map(|_| H::connect()).collect();
        for conn in &conns {
            conn.say("привет").await;
        }
        assert_eq!(metrics.num_alive_tasks(), baseline + 10, "десять подключений: десять фоновых задач");
        drop(conns);
        sleep(Duration::from_millis(200)).await;
        let leaked = metrics.num_alive_tasks() - baseline;
        assert_eq!(leaked, 0, "после отключения всех клиентов и паузы на хвосты живых задач осталось: {leaked}");
    });
}

fn main() {
    suite::<Good>();
    println!("Good проходит");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_suite_passes_on_the_correct_hub() {
        assert_eq!(verdict(20, suite::<Good>), Verdict::Passed, "на исправном хабе тест не должен поднимать ложную тревогу");
    }

    #[test]
    fn the_suite_notices_the_leak() {
        assert_eq!(verdict(20, suite::<Leaky>), Verdict::Failed, "в Leaky задачи переживают отключение: тест обязан это заметить");
    }
}
