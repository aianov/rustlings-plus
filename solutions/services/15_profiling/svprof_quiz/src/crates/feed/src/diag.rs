use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

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
