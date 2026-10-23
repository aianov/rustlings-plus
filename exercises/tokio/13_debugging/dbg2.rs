// Когда в сервисе что-то не отвечает, хочется видеть не только «ждём», но и кого, как долго и
// сколько раз его будили. Нужна обёртка `Traced` над любым future. Она считает опросы, запоминает номер
// задачи (по первому опросу), считает суммарное время, которое future провёл в ожидании (от возврата
// `Pending` до следующего опроса) и помечает, что закончился. Результат future она не меняет.
// TODO: Допиши `poll` для `Traced`.
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::time::Instant;

#[derive(Debug, Clone, Default)]
struct Stats {
    name: &'static str,
    polls: u32,
    pending_for: Duration,
    pending_since: Option<Instant>,
    finished: bool,
    task: Option<tokio::task::Id>,
}

struct Traced<F> {
    inner: Pin<Box<F>>,
    stats: Arc<Mutex<Stats>>,
}

impl<F: Future> Traced<F> {
    fn new(name: &'static str, inner: F) -> (Traced<F>, Arc<Mutex<Stats>>) {
        let stats = Arc::new(Mutex::new(Stats { name, ..Stats::default() }));
        (Traced { inner: Box::pin(inner), stats: stats.clone() }, stats)
    }
}

impl<F: Future> Future for Traced<F> {
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<F::Output> {
        let _ = cx;
        todo!()
    }
}

#[tokio::main]
async fn main() {
    let (traced, stats) = Traced::new("hello", async { 1 });
    println!("{} {:?}", traced.await, stats.lock().unwrap());
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::sleep;

    #[tokio::test(start_paused = true)]
    async fn counts_polls_and_time_spent_waiting() {
        let (traced, stats) = Traced::new("naps", async {
            sleep(Duration::from_millis(100)).await;
            sleep(Duration::from_millis(50)).await;
            7
        });
        assert_eq!(traced.await, 7, "обёртка не должна менять результат");
        let stats = stats.lock().unwrap();
        assert_eq!(stats.name, "naps");
        assert_eq!(stats.polls, 3, "опрос до первого сна, после первого и после второго");
        assert_eq!(stats.pending_for, Duration::from_millis(150), "в ожидании future провёл 100 + 50 мс");
        assert!(stats.finished, "future закончился");
        assert!(stats.pending_since.is_none(), "закончившийся future ничего не ждёт");
    }

    #[tokio::test(start_paused = true)]
    async fn a_ready_future_is_one_poll_and_no_waiting() {
        let (traced, stats) = Traced::new("now", async { "сразу" });
        assert_eq!(traced.await, "сразу");
        let stats = stats.lock().unwrap();
        assert_eq!((stats.polls, stats.pending_for), (1, Duration::ZERO), "готов с первого опроса: один опрос, ожидания нет");
    }

    #[tokio::test(start_paused = true)]
    async fn a_waiting_future_shows_since_when() {
        let (traced, stats) = Traced::new("stuck", std::future::pending::<()>());
        let task = tokio::spawn(traced);
        sleep(Duration::from_millis(10)).await;
        let started_waiting = {
            let stats = stats.lock().unwrap();
            assert_eq!(stats.polls, 1, "future без пробуждений опрошен один раз");
            assert!(!stats.finished, "future ещё ждёт");
            stats.pending_since.expect("future ждёт: момент начала ожидания должен быть записан")
        };
        sleep(Duration::from_millis(40)).await;
        assert_eq!(Instant::now() - started_waiting, Duration::from_millis(50), "ожидание началось 50 мс назад");
        task.abort();
    }

    #[tokio::test(start_paused = true)]
    async fn remembers_the_task_that_polled_it() {
        let (traced, stats) = Traced::new("job", async { 1 });
        let handle = tokio::spawn(traced);
        let id = handle.id();
        handle.await.unwrap();
        assert_eq!(stats.lock().unwrap().task, Some(id), "в статистике должен быть номер задачи, которая опрашивала future");
    }
}
