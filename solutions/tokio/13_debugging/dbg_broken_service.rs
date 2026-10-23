use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use tokio::sync::{Mutex, Notify, mpsc};

struct Session {
    id: u32,
    tx: mpsc::Sender<String>,
}

struct Service {
    ready: AtomicBool,
    ready_signal: Notify,
    balances: Vec<Mutex<i64>>,
    sessions: Arc<StdMutex<HashMap<u32, mpsc::Sender<String>>>>,
    next_session: AtomicU32,
}

async fn session_loop(mut rx: mpsc::Receiver<String>) {
    while let Some(_message) = rx.recv().await {
        tokio::task::yield_now().await;
    }
}

impl Service {
    fn new(balances: &[i64]) -> Service {
        Service {
            ready: AtomicBool::new(false),
            ready_signal: Notify::new(),
            balances: balances.iter().map(|b| Mutex::new(*b)).collect(),
            sessions: Arc::new(StdMutex::new(HashMap::new())),
            next_session: AtomicU32::new(1),
        }
    }

    fn set_ready(&self) {
        self.ready.store(true, Ordering::SeqCst);
        self.ready_signal.notify_waiters();
    }

    fn open_sessions(&self) -> usize {
        self.sessions.lock().unwrap().len()
    }

    async fn balance(&self, id: usize) -> i64 {
        *self.balances[id].lock().await
    }

    async fn wait_ready(&self) {
        loop {
            let notified = self.ready_signal.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.ready.load(Ordering::SeqCst) {
                return;
            }
            notified.await;
        }
    }

    async fn transfer(&self, from: usize, to: usize, amount: i64) {
        if from == to {
            return;
        }
        let (first, second) = if from < to { (from, to) } else { (to, from) };
        let mut low = self.balances[first].lock().await;
        tokio::task::yield_now().await;
        let mut high = self.balances[second].lock().await;
        if from < to {
            *low -= amount;
            *high += amount;
        } else {
            *high -= amount;
            *low += amount;
        }
    }

    fn open_session(&self) -> Session {
        let id = self.next_session.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = mpsc::channel(4);
        self.sessions.lock().unwrap().insert(id, tx.clone());
        tokio::spawn(session_loop(rx));
        Session { id, tx }
    }

    fn close_session(&self, session: Session) {
        self.sessions.lock().unwrap().remove(&session.id);
        drop(session.tx);
    }
}

#[tokio::main]
async fn main() {
    let service = Service::new(&[10, 20]);
    service.set_ready();
    service.wait_ready().await;
    println!("{}", service.balance(0).await);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::runtime::Handle;
    use tokio::time::{sleep, timeout};

    #[tokio::test(start_paused = true)]
    async fn waiting_for_readiness_works_in_both_orders() {
        let service = Arc::new(Service::new(&[0]));
        let waiter = {
            let service = service.clone();
            tokio::spawn(async move { service.wait_ready().await })
        };
        sleep(Duration::from_millis(10)).await;
        service.set_ready();
        timeout(Duration::from_secs(1), waiter).await.expect("ждущий не проснулся после set_ready").unwrap();
        timeout(Duration::from_secs(1), service.wait_ready()).await.expect("готовность уже была, а ждущий, пришедший позже, зависает");
    }

    #[tokio::test(start_paused = true)]
    async fn several_waiters_are_all_released() {
        let service = Arc::new(Service::new(&[0]));
        let waiters: Vec<_> = (0..3)
            .map(|_| {
                let service = service.clone();
                tokio::spawn(async move { service.wait_ready().await })
            })
            .collect();
        sleep(Duration::from_millis(10)).await;
        service.set_ready();
        for waiter in waiters {
            timeout(Duration::from_secs(1), waiter).await.expect("один из ждущих остался висеть").unwrap();
        }
    }

    #[tokio::test(start_paused = true)]
    async fn opposite_transfers_do_not_deadlock() {
        let service = Arc::new(Service::new(&[100, 100]));
        let (s1, s2) = (service.clone(), service.clone());
        let a = tokio::spawn(async move { s1.transfer(0, 1, 30).await });
        let b = tokio::spawn(async move { s2.transfer(1, 0, 10).await });
        timeout(Duration::from_secs(1), async {
            a.await.unwrap();
            b.await.unwrap();
        })
        .await
        .expect("встречные переводы встали друг против друга");
        assert_eq!((service.balance(0).await, service.balance(1).await), (80, 120), "итоговые балансы после двух переводов");
    }

    #[tokio::test(start_paused = true)]
    async fn closed_sessions_leave_nothing_behind() {
        let metrics = Handle::current().metrics();
        let baseline = metrics.num_alive_tasks();
        let service = Service::new(&[0]);
        let sessions: Vec<_> = (0..5).map(|_| service.open_session()).collect();
        for session in &sessions {
            session.tx.send("привет".to_string()).await.unwrap();
        }
        assert_eq!(service.open_sessions(), 5);
        for session in sessions {
            service.close_session(session);
        }
        sleep(Duration::from_millis(100)).await;
        assert_eq!(service.open_sessions(), 0, "после закрытия сеансов в реестре не должно быть записей");
        assert_eq!(metrics.num_alive_tasks() - baseline, 0, "после закрытия сеансов фоновые задачи должны завершиться");
    }
}
