use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

#[derive(Default)]
struct Gauge {
    running: AtomicUsize,
    peak: AtomicUsize,
}

async fn work(id: u32, gauge: &Gauge) -> u32 {
    let now = gauge.running.fetch_add(1, Ordering::SeqCst) + 1;
    gauge.peak.fetch_max(now, Ordering::SeqCst);
    tokio::time::sleep(Duration::from_millis(10)).await;
    gauge.running.fetch_sub(1, Ordering::SeqCst);
    id * 3
}

async fn fetch_all(ids: Vec<u32>, limit: usize, gauge: Arc<Gauge>) -> Vec<u32> {
    let permits = Arc::new(tokio::sync::Semaphore::new(limit));
    let handles: Vec<_> = ids
        .into_iter()
        .map(|id| {
            let gauge = Arc::clone(&gauge);
            let permits = Arc::clone(&permits);
            tokio::spawn(async move {
                let _permit = permits.acquire().await.unwrap();
                work(id, &gauge).await
            })
        })
        .collect();
    let mut out = Vec::new();
    for handle in handles {
        out.push(handle.await.unwrap());
    }
    out
}

#[tokio::main]
async fn main() {
    let gauge = Arc::new(Gauge::default());
    println!("{:?}", fetch_all(vec![1, 2, 3], 2, gauge).await);
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::Instant;

    #[tokio::test(start_paused = true)]
    async fn never_more_than_limit() {
        let gauge = Arc::new(Gauge::default());
        let out = fetch_all((0..30).collect(), 4, Arc::clone(&gauge)).await;
        assert_eq!(out, (0..30).map(|id| id * 3).collect::<Vec<_>>(), "результаты по порядку входа");
        assert_eq!(gauge.peak.load(Ordering::SeqCst), 4, "одновременно должно работать ровно limit запросов");
    }

    #[tokio::test(start_paused = true)]
    async fn the_slots_are_reused() {
        let gauge = Arc::new(Gauge::default());
        let started = Instant::now();
        fetch_all((0..8).collect(), 4, Arc::clone(&gauge)).await;
        assert_eq!(started.elapsed(), Duration::from_millis(20), "8 запросов по 10 мс при лимите 4: две волны");
    }

    #[tokio::test(start_paused = true)]
    async fn limit_one() {
        let gauge = Arc::new(Gauge::default());
        fetch_all((0..5).collect(), 1, Arc::clone(&gauge)).await;
        assert_eq!(gauge.peak.load(Ordering::SeqCst), 1, "limit = 1 это строго по одному");
    }
}
