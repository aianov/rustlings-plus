use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use crate::metrics::metrics;

pub const SLOW_POLL: Duration = Duration::from_millis(10);

pub const SLOW_POLLS: &str = "tokio_slow_polls_total";

pub struct Watched<F> {
    name: &'static str,
    limit: Duration,
    inner: Pin<Box<F>>,
}

pub fn watch_polls<F: Future>(name: &'static str, future: F) -> Watched<F> {
    watch_polls_with(name, SLOW_POLL, future)
}

pub fn watch_polls_with<F: Future>(name: &'static str, limit: Duration, future: F) -> Watched<F> {
    Watched {
        name,
        limit,
        inner: Box::pin(future),
    }
}

impl<F: Future> Future for Watched<F> {
    type Output = F::Output;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<F::Output> {
        let started = Instant::now();
        let polled = self.inner.as_mut().poll(cx);
        let spent = started.elapsed();
        if spent > self.limit {
            let name = self.name;
            metrics().inc(
                SLOW_POLLS,
                "poll дольше порога: внутри блокирующий вызов или тяжёлый расчёт",
                &[("task", name)],
                1,
            );
            tracing::warn!(
                task = name,
                poll_ms = u64::try_from(spent.as_millis()).unwrap_or(u64::MAX),
                "долгий poll: блокирующее место просится в spawn_blocking"
            );
        }
        polled
    }
}
