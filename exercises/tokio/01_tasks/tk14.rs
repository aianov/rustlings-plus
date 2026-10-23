// Десять тысяч задач разом это десять тысяч соединений, файлов или запросов к базе. Часто
// нужно «не больше N одновременно». Сейчас run_limited запускает всё сразу, и пик
// одновременных задач равен числу элементов. Как только одна задача закончилась, на её место
// должна встать следующая, не дожидаясь остальных.
// TODO: Перепиши run_limited. Результаты верни отсортированными. limit не меньше 1.
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio::task::JoinSet;

#[derive(Default)]
struct Gauge {
    running: AtomicUsize,
    peak: AtomicUsize,
}

async fn process(n: u32, gauge: Arc<Gauge>) -> u32 {
    let now = gauge.running.fetch_add(1, Ordering::SeqCst) + 1;
    gauge.peak.fetch_max(now, Ordering::SeqCst);
    tokio::time::sleep(Duration::from_millis(10)).await;
    gauge.running.fetch_sub(1, Ordering::SeqCst);
    n * 2
}

async fn run_limited(items: Vec<u32>, limit: usize, gauge: Arc<Gauge>) -> Vec<u32> {
    let mut set = JoinSet::new();
    for n in items {
        set.spawn(process(n, Arc::clone(&gauge)));
    }
    let mut out = Vec::new();
    while let Some(result) = set.join_next().await {
        out.push(result.unwrap());
    }
    out.sort();
    out
}

#[tokio::main]
async fn main() {
    let gauge = Arc::new(Gauge::default());
    let out = run_limited((0..10).collect(), 3, Arc::clone(&gauge)).await;
    println!("{out:?}, пик {}", gauge.peak.load(Ordering::SeqCst));
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::Instant;

    #[tokio::test(start_paused = true)]
    async fn never_more_than_limit() {
        let gauge = Arc::new(Gauge::default());
        let out = run_limited((0..10).collect(), 3, Arc::clone(&gauge)).await;
        assert_eq!(out, [0, 2, 4, 6, 8, 10, 12, 14, 16, 18], "все элементы обработаны");
        assert_eq!(gauge.peak.load(Ordering::SeqCst), 3, "одновременно должно идти ровно limit задач, не больше и не меньше");
    }

    #[tokio::test(start_paused = true)]
    async fn limit_one_is_sequential() {
        let gauge = Arc::new(Gauge::default());
        let started = Instant::now();
        run_limited((0..5).collect(), 1, Arc::clone(&gauge)).await;
        assert_eq!(gauge.peak.load(Ordering::SeqCst), 1, "limit = 1: задачи строго по одной");
        assert_eq!(started.elapsed(), Duration::from_millis(50), "5 задач по 10 мс подряд");
    }

    #[tokio::test(start_paused = true)]
    async fn window_slides_without_waiting_for_the_whole_batch() {
        let gauge = Arc::new(Gauge::default());
        let started = Instant::now();
        run_limited((0..9).collect(), 3, Arc::clone(&gauge)).await;
        assert_eq!(started.elapsed(), Duration::from_millis(30), "9 задач по 3 за раз: три волны по 10 мс");
    }

    #[tokio::test(start_paused = true)]
    async fn limit_bigger_than_items() {
        let gauge = Arc::new(Gauge::default());
        run_limited((0..4).collect(), 100, Arc::clone(&gauge)).await;
        assert_eq!(gauge.peak.load(Ordering::SeqCst), 4, "лимит больше числа элементов: идут все сразу");
    }
}
