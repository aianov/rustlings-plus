use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

struct Item {
    depth: u32,
}

#[derive(Clone)]
struct Inbox(mpsc::Sender<Item>);

impl Inbox {
    async fn send(&self, item: Item) {
        self.0.send(item).await.unwrap();
    }
}

async fn worker(mut inbox: mpsc::Receiver<Item>, peer: mpsc::Sender<Item>, processed: Arc<AtomicU32>) {
    let mut outbox: std::collections::VecDeque<Item> = std::collections::VecDeque::new();
    loop {
        tokio::select! {
            permit = peer.reserve(), if !outbox.is_empty() => {
                match permit {
                    Ok(permit) => permit.send(outbox.pop_front().expect("очередь не пуста")),
                    Err(_) => return,
                }
            }
            item = inbox.recv() => match item {
                Some(item) => {
                    processed.fetch_add(1, Ordering::SeqCst);
                    if item.depth > 0 {
                        outbox.push_back(Item { depth: item.depth - 1 });
                        outbox.push_back(Item { depth: item.depth - 1 });
                    }
                }
                None => return,
            },
        }
    }
}

fn start(capacity: usize) -> (Inbox, Inbox, Arc<AtomicU32>, Vec<JoinHandle<()>>) {
    let (a_tx, a_rx) = mpsc::channel(capacity);
    let (b_tx, b_rx) = mpsc::channel(capacity);
    let processed = Arc::new(AtomicU32::new(0));
    let a = tokio::spawn(worker(a_rx, b_tx.clone(), processed.clone()));
    let b = tokio::spawn(worker(b_rx, a_tx.clone(), processed.clone()));
    (Inbox(a_tx), Inbox(b_tx), processed, vec![a, b])
}

#[tokio::main]
async fn main() {
    let (a, _b, processed, _tasks) = start(8);
    a.send(Item { depth: 1 }).await;
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    println!("{}", processed.load(Ordering::SeqCst));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::{sleep, timeout};

    async fn run_with(capacity: usize) {
        let (a, b, processed, tasks) = start(capacity);
        a.send(Item { depth: 3 }).await;
        b.send(Item { depth: 3 }).await;
        let waited = timeout(Duration::from_secs(5), async {
            while processed.load(Ordering::SeqCst) < 30 {
                sleep(Duration::from_millis(10)).await;
            }
        })
        .await;
        let seen = processed.load(Ordering::SeqCst);
        assert!(waited.is_ok(), "ящик на {capacity}: обработано {seen} из 30, акторы встали, ожидая места в ящиках друг друга");
        sleep(Duration::from_secs(1)).await;
        assert_eq!(processed.load(Ordering::SeqCst), 30, "ящик на {capacity}: порции не должны ни теряться, ни задваиваться");
        for task in tasks {
            task.abort();
        }
    }

    #[tokio::test(start_paused = true)]
    async fn a_tiny_mailbox_does_not_deadlock() {
        run_with(1).await;
    }

    #[tokio::test(start_paused = true)]
    async fn a_small_mailbox_does_not_deadlock() {
        run_with(2).await;
    }

    #[tokio::test(start_paused = true)]
    async fn a_roomy_mailbox_works_as_before() {
        run_with(64).await;
    }
}
