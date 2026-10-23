use std::future::Future;
use std::sync::Arc;
use tokio::sync::watch;
use tokio::task::JoinHandle;

#[derive(Clone)]
struct Tracker {
    running: Arc<watch::Sender<usize>>,
}

struct Exit(Arc<watch::Sender<usize>>);

impl Drop for Exit {
    fn drop(&mut self) {
        self.0.send_modify(|count| *count -= 1);
    }
}

impl Tracker {
    fn new() -> Tracker {
        Tracker { running: Arc::new(watch::channel(0).0) }
    }

    fn spawn<F>(&self, task: F) -> JoinHandle<()>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        self.running.send_modify(|count| *count += 1);
        let exit = Exit(self.running.clone());
        tokio::spawn(async move {
            let _exit = exit;
            task.await;
        })
    }

    fn len(&self) -> usize {
        *self.running.borrow()
    }

    async fn wait(&self) {
        let mut rx = self.running.subscribe();
        let _ = rx.wait_for(|count| *count == 0).await;
    }
}

#[tokio::main]
async fn main() {
    let tracker = Tracker::new();
    tracker.spawn(async {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    });
    tracker.wait().await;
    println!("{}", tracker.len());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::{Instant, sleep, timeout};

    #[tokio::test(start_paused = true)]
    async fn wait_returns_when_the_last_task_is_done() {
        let tracker = Tracker::new();
        for ms in [100, 200, 300] {
            tracker.spawn(async move { sleep(Duration::from_millis(ms)).await });
        }
        assert_eq!(tracker.len(), 3, "три задачи запущены, и учёт видит их сразу, не дожидаясь первого опроса");
        let start = Instant::now();
        tracker.wait().await;
        assert_eq!(start.elapsed(), Duration::from_millis(300), "wait ждёт самую долгую задачу");
        assert_eq!(tracker.len(), 0, "после wait задач не осталось");
    }

    #[tokio::test(start_paused = true)]
    async fn wait_without_tasks_returns_at_once() {
        let tracker = Tracker::new();
        let start = Instant::now();
        timeout(Duration::from_secs(1), tracker.wait()).await.expect("задач нет: wait не должен ждать");
        assert_eq!(start.elapsed(), Duration::ZERO, "пустой учёт не задерживает");
    }

    #[tokio::test(start_paused = true)]
    async fn tasks_spawned_by_tasks_are_waited_for_too() {
        let tracker = Tracker::new();
        let inner = tracker.clone();
        tracker.spawn(async move {
            sleep(Duration::from_millis(50)).await;
            inner.spawn(async { sleep(Duration::from_millis(100)).await });
        });
        let start = Instant::now();
        tracker.wait().await;
        assert_eq!(start.elapsed(), Duration::from_millis(150), "порождённая задача живёт ещё 100 мс, wait обязан дождаться и её");
    }

    #[tokio::test(start_paused = true)]
    async fn panic_and_abort_also_end_a_tasks_life() {
        let tracker = Tracker::new();
        tracker.spawn(async { panic!("задача упала") });
        let forever = tracker.spawn(async { std::future::pending::<()>().await });
        let never_polled = tracker.spawn(async { std::future::pending::<()>().await });
        sleep(Duration::from_millis(10)).await;
        assert_eq!(tracker.len(), 2, "упавшая задача на учёте остаться не может");
        forever.abort();
        never_polled.abort();
        timeout(Duration::from_secs(1), tracker.wait())
            .await
            .expect("задачи отменены, а учёт продолжает считать их живыми");
        assert_eq!(tracker.len(), 0, "после abort учёт пуст");
    }
}
