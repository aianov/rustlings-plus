use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::sleep;

#[derive(Debug, Clone, PartialEq)]
struct Notification {
    user: u32,
    seq: u32,
}

type Log = Arc<Mutex<Vec<(usize, Notification)>>>;

async fn deliver(worker: usize, mut inbox: mpsc::Receiver<Notification>, log: Log) {
    while let Some(n) = inbox.recv().await {
        sleep(Duration::from_millis(10)).await;
        log.lock().unwrap().push((worker, n));
    }
}

struct Dispatcher {
    inboxes: Vec<mpsc::Sender<Notification>>,
    tasks: Vec<JoinHandle<()>>,
}

impl Dispatcher {
    fn new(workers: usize, log: Log) -> Dispatcher {
        let mut inboxes = Vec::new();
        let mut tasks = Vec::new();
        for id in 0..workers {
            let (tx, rx) = mpsc::channel(64);
            tasks.push(tokio::spawn(deliver(id, rx, log.clone())));
            inboxes.push(tx);
        }
        Dispatcher { inboxes, tasks }
    }

    async fn send(&self, n: Notification) {
        let worker = n.user as usize % self.inboxes.len();
        self.inboxes[worker].send(n).await.expect("воркер доставки остановился");
    }

    async fn finish(self) {
        drop(self.inboxes);
        for task in self.tasks {
            task.await.unwrap();
        }
    }
}

#[tokio::main]
async fn main() {
    let log: Log = Arc::default();
    let dispatcher = Dispatcher::new(2, log.clone());
    dispatcher.send(Notification { user: 1, seq: 0 }).await;
    dispatcher.finish().await;
    println!("{:?}", log.lock().unwrap());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use tokio::time::Instant;

    async fn run_load(workers: usize, users: u32, per_user: u32) -> (Vec<(usize, Notification)>, Duration) {
        let log: Log = Arc::default();
        let dispatcher = Dispatcher::new(workers, log.clone());
        let start = Instant::now();
        for seq in 0..per_user {
            for user in 0..users {
                dispatcher.send(Notification { user, seq }).await;
            }
        }
        dispatcher.finish().await;
        let elapsed = start.elapsed();
        let entries = log.lock().unwrap().clone();
        (entries, elapsed)
    }

    #[tokio::test(start_paused = true)]
    async fn each_user_is_served_by_one_worker_in_order() {
        let (entries, _) = run_load(4, 8, 5).await;
        assert_eq!(entries.len(), 40, "восемь пользователей по пять уведомлений: все сорок должны быть доставлены");
        let mut by_user: HashMap<u32, Vec<(usize, u32)>> = HashMap::new();
        for (worker, n) in &entries {
            by_user.entry(n.user).or_default().push((*worker, n.seq));
        }
        for (user, got) in by_user {
            let workers: Vec<usize> = got.iter().map(|(w, _)| *w).collect();
            assert!(workers.iter().all(|w| *w == workers[0]), "уведомления пользователя {user} разошлись по воркерам {workers:?}: порядок не удержать");
            let seqs: Vec<u32> = got.iter().map(|(_, s)| *s).collect();
            assert_eq!(seqs, vec![0, 1, 2, 3, 4], "у пользователя {user} порядок доставки нарушен");
        }
    }

    #[tokio::test(start_paused = true)]
    async fn different_users_are_served_in_parallel() {
        let (entries, elapsed) = run_load(4, 8, 5).await;
        let used: std::collections::HashSet<usize> = entries.iter().map(|(w, _)| *w).collect();
        assert!(used.len() >= 2, "все уведомления достались одному воркеру: параллельности нет");
        assert!(
            elapsed < Duration::from_millis(300),
            "сорок уведомлений по 10 мс на четырёх воркерах не должны занять {elapsed:?}: последовательно было бы 400 мс"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn finish_waits_for_everything_queued() {
        let log: Log = Arc::default();
        let dispatcher = Dispatcher::new(3, log.clone());
        for seq in 0..30 {
            dispatcher.send(Notification { user: seq % 7, seq }).await;
        }
        dispatcher.finish().await;
        assert_eq!(log.lock().unwrap().len(), 30, "после finish все отправленные уведомления уже доставлены");
    }
}
