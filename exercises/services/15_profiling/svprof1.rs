// Сервис под нагрузкой отвечает рывками, а в метриках процессор не загружен. Подозрение: где-то внутри async-кода есть вызов,
// который не отдаёт управление рантайму: пока он работает, воркер tokio ничего другого не делает, и все чужие задачи на нём ждут.
// Такое не видно ни в каком журнале, потому что функции формально «выполняются быстро». Нужен сторож: обёртка над future, которая
// замеряет длительность каждого `poll`. Цель: `watch` оборачивает future так, чтобы результат остался тем же, а статистика
// (число опросов, число опросов дольше порога, самый долгий) была доступна и во время работы, и после.


use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

fn main() {}

#[derive(Default)]
pub struct PollStats {
    polls: AtomicU64,
    slow: AtomicU64,
    longest_nanos: AtomicU64,
}

impl PollStats {
    pub fn polls(&self) -> u64 {
        self.polls.load(Ordering::SeqCst)
    }

    pub fn slow_polls(&self) -> u64 {
        self.slow.load(Ordering::SeqCst)
    }

    pub fn longest(&self) -> Duration {
        Duration::from_nanos(self.longest_nanos.load(Ordering::SeqCst))
    }
}

pub struct Watched<F> {
    inner: Pin<Box<F>>,
    threshold: Duration,
    stats: Arc<PollStats>,
}

pub fn watch<F: Future>(inner: F, threshold: Duration) -> (Watched<F>, Arc<PollStats>) {
    let stats = Arc::new(PollStats::default());
    (Watched { inner: Box::pin(inner), threshold, stats: stats.clone() }, stats)
}

impl<F: Future> Future for Watched<F> {
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<F::Output> {
        let _ = (&self.inner, &self.threshold, &self.stats, cx, Instant::now());
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::poll_fn;

    const MS: fn(u64) -> Duration = Duration::from_millis;

    fn staged(blocking_at: &'static [u32], block_for: Duration, until: u32) -> impl Future<Output = u32> {
        let mut n = 0;
        poll_fn(move |cx| {
            n += 1;
            if blocking_at.contains(&n) {
                std::thread::sleep(block_for);
            }
            if n < until {
                cx.waker().wake_by_ref();
                Poll::Pending
            } else {
                Poll::Ready(n)
            }
        })
    }

    #[tokio::test]
    async fn the_result_is_passed_through_and_every_poll_is_counted() {
        let (fut, stats) = watch(staged(&[], MS(0), 4), MS(10));
        assert_eq!(fut.await, 4, "результат обёрнутой future не должен меняться");
        assert_eq!(stats.polls(), 4, "три раза Pending и один Ready: четыре опроса");
        assert_eq!(stats.slow_polls(), 0);
    }

    #[tokio::test]
    async fn a_poll_that_blocks_is_found_and_the_quick_ones_are_not() {
        let (fut, stats) = watch(staged(&[2], MS(60), 5), MS(30));
        fut.await;
        assert_eq!(stats.polls(), 5);
        assert_eq!(stats.slow_polls(), 1, "ровно один опрос заблокировал поток на 60 мс при пороге 30");
        assert!(stats.longest() >= MS(60), "самый долгий опрос не короче 60 мс: {:?}", stats.longest());
    }

    #[tokio::test]
    async fn waiting_is_not_blocking() {
        let (fut, stats) = watch(
            async {
                for _ in 0..3 {
                    tokio::time::sleep(MS(40)).await;
                }
            },
            MS(20),
        );
        fut.await;
        assert_eq!(stats.slow_polls(), 0, "ожидание на таймере занимает 120 мс, но сам опрос короткий: ждёт задача, а не поток");
        assert!(stats.longest() < MS(20), "{:?}", stats.longest());
    }

    #[tokio::test]
    async fn statistics_are_visible_while_the_future_is_still_running() {
        let (fut, stats) = watch(staged(&[1], MS(40), 3), MS(10));
        let seen = Arc::new(AtomicU64::new(0));
        let (stats2, seen2) = (stats.clone(), seen.clone());
        let task = tokio::spawn(async move {
            fut.await;
            seen2.store(stats2.polls(), Ordering::SeqCst);
        });
        task.await.expect("задача не упала");
        assert_eq!(seen.load(Ordering::SeqCst), 3);
        assert_eq!(stats.slow_polls(), 1);
    }

    #[tokio::test]
    async fn blocking_in_a_task_delays_a_neighbour_on_the_same_thread() {
        let ticks = Arc::new(AtomicU64::new(0));
        let t2 = ticks.clone();
        let neighbour = tokio::spawn(async move {
            loop {
                tokio::time::sleep(MS(5)).await;
                t2.fetch_add(1, Ordering::SeqCst);
            }
        });
        tokio::time::sleep(MS(30)).await;
        let before = ticks.load(Ordering::SeqCst);
        let (blocker, stats) = watch(async { std::thread::sleep(MS(150)) }, MS(50));
        blocker.await;
        let during = ticks.load(Ordering::SeqCst) - before;
        neighbour.abort();
        assert_eq!(stats.slow_polls(), 1);
        assert!(during <= 1, "за 150 мс блокировки сосед не успел сделать ни одного шага (шагов {during}): на текущем потоке стоит всё");
    }
}
