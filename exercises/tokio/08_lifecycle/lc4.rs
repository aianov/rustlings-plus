// Чтобы остановить сервис красиво, надо знать, какие задачи ещё работают: запустил десяток и ждёшь,
// пока закончится последняя. Хранить для этого все `JoinHandle` неудобно (задачи сами запускают
// задачи), да и не обязательно. Нужен учёт: счётчик живых задач и ожидание нуля.
// TODO: Допиши `Tracker`: `spawn` запускает задачу и берёт её на учёт, `len` отвечает, сколько задач живо,
// `wait` дожидается, пока их не останется. Задача считается живой с момента запуска, а не первого опроса,
// и перестаёт ей быть при любом конце: возврат, паника, `abort`.
use std::future::Future;
use std::sync::Arc;
use tokio::sync::watch;
use tokio::task::JoinHandle;

#[derive(Clone)]
struct Tracker;

impl Tracker {
    fn new() -> Tracker {
        todo!()
    }

    fn spawn<F>(&self, task: F) -> JoinHandle<()>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        todo!()
    }

    fn len(&self) -> usize {
        todo!()
    }

    async fn wait(&self) {
        todo!()
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
