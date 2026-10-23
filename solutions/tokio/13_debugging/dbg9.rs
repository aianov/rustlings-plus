use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

enum Cmd {
    Add(u32),
    Total(oneshot::Sender<u32>),
    Tick,
}

#[derive(Clone)]
struct Counter {
    tx: mpsc::Sender<Cmd>,
}

impl Counter {
    async fn add(&self, n: u32) {
        self.tx.send(Cmd::Add(n)).await.unwrap();
    }

    async fn total(&self) -> u32 {
        let (reply, answer) = oneshot::channel();
        self.tx.send(Cmd::Total(reply)).await.unwrap();
        answer.await.unwrap()
    }
}

fn start() -> (Counter, JoinHandle<()>, Arc<AtomicU32>) {
    let (tx, rx) = mpsc::channel(8);
    let ticks = Arc::new(AtomicU32::new(0));
    let me = tx.downgrade();
    let task = tokio::spawn(run(rx, me, ticks.clone()));
    (Counter { tx }, task, ticks)
}

async fn run(mut rx: mpsc::Receiver<Cmd>, me: mpsc::WeakSender<Cmd>, ticks: Arc<AtomicU32>) {
    let mut total = 0;
    while let Some(cmd) = rx.recv().await {
        match cmd {
            Cmd::Add(n) => {
                total += n;
                if let Some(me) = me.upgrade() {
                    let _ = me.try_send(Cmd::Tick);
                }
            }
            Cmd::Tick => {
                ticks.fetch_add(1, Ordering::SeqCst);
            }
            Cmd::Total(reply) => {
                let _ = reply.send(total);
            }
        }
    }
}

#[tokio::main]
async fn main() {
    let (counter, task, ticks) = start();
    counter.add(2).await;
    drop(counter);
    task.await.unwrap();
    println!("{}", ticks.load(Ordering::SeqCst));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::{sleep, timeout};

    #[tokio::test(start_paused = true)]
    async fn ticks_are_scheduled_while_the_owners_live() {
        let (counter, _task, ticks) = start();
        counter.add(1).await;
        counter.add(2).await;
        assert_eq!(counter.total().await, 3, "сумма пополнений");
        sleep(Duration::from_millis(10)).await;
        assert_eq!(ticks.load(Ordering::SeqCst), 2, "каждое пополнение ставит один тик, актор их обрабатывает");
    }

    #[tokio::test(start_paused = true)]
    async fn the_actor_ends_when_all_owners_leave() {
        let (counter, task, ticks) = start();
        let second = counter.clone();
        counter.add(5).await;
        drop(counter);
        sleep(Duration::from_millis(10)).await;
        assert!(!task.is_finished(), "один владелец остался: актор обязан жить");
        second.add(1).await;
        assert_eq!(second.total().await, 6, "оба пополнения учтены");
        drop(second);
        timeout(Duration::from_secs(1), task).await.expect("владельцев нет, а задача актора не завершается").unwrap();
        assert_eq!(ticks.load(Ordering::SeqCst), 2, "оба тика обработаны до выхода");
    }

    #[tokio::test(start_paused = true)]
    async fn an_idle_actor_ends_too() {
        let (counter, task, _ticks) = start();
        drop(counter);
        timeout(Duration::from_secs(1), task).await.expect("актор без единой команды тоже должен завершиться").unwrap();
    }
}
