use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::Notify;

struct Ready {
    flag: AtomicBool,
    notify: Notify,
}

impl Ready {
    fn new() -> Self {
        Ready { flag: AtomicBool::new(false), notify: Notify::new() }
    }
}

impl Ready {
    fn set_ready(&self) {
        self.flag.store(true, Ordering::SeqCst);
        self.notify.notify_waiters();
    }

    async fn wait_ready(&self) {
        loop {
            let notified = self.notify.notified();
            if self.flag.load(Ordering::SeqCst) {
                return;
            }
            notified.await;
        }
    }
}

#[tokio::main]
async fn main() {
    let ready = Ready::new();
    ready.set_ready();
    ready.wait_ready().await;
    println!("готов");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::time::timeout;

    fn start_waiter(ready: &Arc<Ready>) -> tokio::task::JoinHandle<()> {
        let ready = Arc::clone(ready);
        tokio::spawn(async move { ready.wait_ready().await })
    }

    #[tokio::test(start_paused = true)]
    async fn ready_before_waiting() {
        let ready = Ready::new();
        ready.set_ready();
        assert!(
            timeout(Duration::from_secs(1), ready.wait_ready()).await.is_ok(),
            "готовность объявили до начала ожидания, и она пропала: ждущая задача зависла"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn all_early_waiters_wake_up() {
        let ready = Arc::new(Ready::new());
        let waiters: Vec<_> = (0..3).map(|_| start_waiter(&ready)).collect();
        tokio::time::sleep(Duration::from_millis(10)).await;
        ready.set_ready();
        for waiter in waiters {
            assert!(
                timeout(Duration::from_secs(1), waiter).await.is_ok(),
                "готовность должна разбудить всех, кто уже ждал, а не одну задачу"
            );
        }
    }

    #[tokio::test(start_paused = true)]
    async fn late_waiters_pass_straight_through() {
        let ready = Arc::new(Ready::new());
        let early = start_waiter(&ready);
        tokio::time::sleep(Duration::from_millis(10)).await;
        ready.set_ready();
        assert!(timeout(Duration::from_secs(1), early).await.is_ok(), "ранняя задача должна проснуться");
        for _ in 0..3 {
            let late = start_waiter(&ready);
            assert!(
                timeout(Duration::from_secs(1), late).await.is_ok(),
                "готовность объявлена давно и навсегда: опоздавшая задача не должна ждать"
            );
        }
    }

    #[tokio::test(start_paused = true)]
    async fn nobody_passes_before_ready() {
        let ready = Arc::new(Ready::new());
        let waiter = start_waiter(&ready);
        tokio::time::sleep(Duration::from_secs(5)).await;
        assert!(!waiter.is_finished(), "готовности не было, а ждущая задача уже ушла дальше");
        ready.set_ready();
        assert!(timeout(Duration::from_secs(1), waiter).await.is_ok(), "после set_ready задача должна проснуться");
    }
}
