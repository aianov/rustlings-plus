// Сердцебиение должно стучать каждые 10 мс. Рядом работает фоновый расчёт `crunch` на 300 мс. Пока
// он считает, сердцебиение молчит, хотя расчёт «асинхронный» и по формальным признакам невиновен.
// Сторож долгих poll (`Watched` из прошлой задачи) уже подключён и укажет виновника. Почини расчёт
// так, чтобы он работал столько же, но не подвешивал соседей, и сторож молчал.
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

#[derive(Debug)]
struct LongPoll {
    name: &'static str,
    took: Duration,
}

struct Watched<F> {
    name: &'static str,
    threshold: Duration,
    inner: Pin<Box<F>>,
    sink: mpsc::UnboundedSender<LongPoll>,
}

impl<F: Future> Future for Watched<F> {
    type Output = F::Output;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<F::Output> {
        let started = Instant::now();
        let out = self.inner.as_mut().poll(cx);
        let took = started.elapsed();
        if took > self.threshold {
            let _ = self.sink.send(LongPoll { name: self.name, took });
        }
        out
    }
}

fn mix(n: u64) -> u64 {
    n.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407) >> 7
}

async fn heartbeat(stop: Arc<AtomicBool>, gaps: Arc<Mutex<Vec<Duration>>>) {
    let mut last = Instant::now();
    while !stop.load(Ordering::SeqCst) {
        tokio::time::sleep(Duration::from_millis(10)).await;
        gaps.lock().unwrap().push(last.elapsed());
        last = Instant::now();
    }
}

async fn crunch(for_ms: u64) -> u64 {
    let end = Instant::now() + Duration::from_millis(for_ms);
    let mut n = 1u64;
    while Instant::now() < end {
        n = mix(n);
    }
    n
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    println!("{}", crunch(5).await);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "current_thread")]
    async fn crunch_works_without_freezing_the_neighbours() {
        let stop = Arc::new(AtomicBool::new(false));
        let gaps = Arc::new(Mutex::new(Vec::new()));
        let beat = tokio::spawn(heartbeat(stop.clone(), gaps.clone()));
        tokio::time::sleep(Duration::from_millis(30)).await;
        let (tx, mut rx) = mpsc::unbounded_channel();
        let started = Instant::now();
        let result = Watched { name: "crunch", threshold: Duration::from_millis(50), inner: Box::pin(crunch(300)), sink: tx }.await;
        let spent = started.elapsed();
        stop.store(true, Ordering::SeqCst);
        beat.await.unwrap();
        assert_ne!(result, 0, "расчёт должен вернуть результат работы");
        assert!(spent >= Duration::from_millis(300), "расчёт должен проработать свои 300 мс, а проработал {spent:?}");
        if let Ok(report) = rx.try_recv() {
            panic!("сторож сообщил: {} держал поток {:?} за один poll", report.name, report.took);
        }
        let worst = gaps.lock().unwrap().iter().copied().max().unwrap_or_default();
        assert!(worst < Duration::from_millis(150), "сердцебиение молчало {worst:?}: расчёт держал поток");
    }
}
