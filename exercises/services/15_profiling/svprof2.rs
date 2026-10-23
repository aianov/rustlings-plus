// Обработчик экспорта читает снимок с медленного сетевого диска и разбирает его: чтение тяжёлое, разбор тоже. Пока один
// пользователь ждёт экспорт, остальные запросы на том же воркере висят, хотя к экспорту отношения не имеют: перестали обновляться
// ленты, замолчали уведомления. Сторож долгих опросов (из `svprof1`, он уже подключён) показывает опросы по сто миллисекунд.
// Цель: обработчик `handle_export` возвращает тот же результат, но ни одна его часть не удерживает поток рантайма дольше нескольких
// миллисекунд, и несколько экспортов одновременно идут параллельно, а не друг за другом. Сами функции чтения и разбора менять нельзя:
// это чужая библиотека, синхронная и медленная.


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
        let this = self.get_mut();
        let started = Instant::now();
        let result = this.inner.as_mut().poll(cx);
        let took = started.elapsed();
        this.stats.polls.fetch_add(1, Ordering::SeqCst);
        if took > this.threshold {
            this.stats.slow.fetch_add(1, Ordering::SeqCst);
        }
        let nanos = u64::try_from(took.as_nanos()).unwrap_or(u64::MAX);
        this.stats.longest_nanos.fetch_max(nanos, Ordering::SeqCst);
        result
    }
}


#[derive(Debug, PartialEq)]
pub struct Report {
    pub lines: usize,
    pub bytes: usize,
}

fn read_snapshot(name: &str) -> Vec<u8> {
    std::thread::sleep(Duration::from_millis(60));
    format!("{name}\n").repeat(1000).into_bytes()
}

fn parse_report(bytes: &[u8]) -> Report {
    std::thread::sleep(Duration::from_millis(60));
    Report { lines: bytes.iter().filter(|b| **b == b'\n').count(), bytes: bytes.len() }
}

pub async fn handle_export(name: &str) -> Report {
    let bytes = read_snapshot(name);
    parse_report(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: fn(u64) -> Duration = Duration::from_millis;

    #[tokio::test]
    async fn the_result_is_the_same() {
        assert_eq!(handle_export("alpha").await, Report { lines: 1000, bytes: 6 * 1000 });
    }

    #[tokio::test]
    async fn no_poll_holds_the_thread() {
        let (fut, stats) = watch(handle_export("alpha"), MS(25));
        fut.await;
        assert_eq!(stats.slow_polls(), 0, "самый долгий опрос длился {:?}: синхронная работа выполняется прямо в async-коде", stats.longest());
    }

    #[tokio::test]
    async fn a_neighbour_task_keeps_ticking_during_the_export() {
        let ticks = Arc::new(AtomicU64::new(0));
        let t2 = ticks.clone();
        let neighbour = tokio::spawn(async move {
            loop {
                tokio::time::sleep(MS(5)).await;
                t2.fetch_add(1, Ordering::SeqCst);
            }
        });
        tokio::time::sleep(MS(20)).await;
        let before = ticks.load(Ordering::SeqCst);
        handle_export("alpha").await;
        let during = ticks.load(Ordering::SeqCst) - before;
        neighbour.abort();
        assert!(during >= 5, "пока шёл экспорт (120 мс), сосед сделал всего {during} шагов: рантайм был занят экспортом");
    }

    #[tokio::test]
    async fn exports_run_in_parallel_not_in_a_queue() {
        let started = Instant::now();
        let (a, b, c, d) = tokio::join!(handle_export("a"), handle_export("b"), handle_export("c"), handle_export("d"));
        assert_eq!((a.lines, b.lines, c.lines, d.lines), (1000, 1000, 1000, 1000));
        let took = started.elapsed();
        assert!(took < MS(330), "четыре экспорта по 120 мс заняли {took:?}: по очереди выйдет около 480 мс, параллельно около 120");
    }
}
