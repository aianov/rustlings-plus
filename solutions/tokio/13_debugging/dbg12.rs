use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
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

impl<F: Future> Watched<F> {
    fn new(name: &'static str, threshold: Duration, inner: F, sink: mpsc::UnboundedSender<LongPoll>) -> Watched<F> {
        Watched { name, threshold, inner: Box::pin(inner), sink }
    }
}

impl<F: Future> Future for Watched<F> {
    type Output = F::Output;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<F::Output> {
        let started = std::time::Instant::now();
        let out = self.inner.as_mut().poll(cx);
        let took = started.elapsed();
        if took > self.threshold {
            let _ = self.sink.send(LongPoll { name: self.name, took });
        }
        out
    }
}

#[tokio::main]
async fn main() {
    let (tx, mut rx) = mpsc::unbounded_channel();
    Watched::new("hello", Duration::from_millis(10), async {}, tx).await;
    println!("{:?}", rx.try_recv().is_err());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[tokio::test]
    async fn a_blocking_poll_is_reported_with_its_name_and_length() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let value = Watched::new("hash", ms(10), async {
            std::thread::sleep(ms(40));
            5
        }, tx)
        .await;
        assert_eq!(value, 5, "обёртка не меняет результат");
        let report = rx.try_recv().expect("poll на 40 мс при пороге 10 мс обязан попасть в отчёт");
        assert_eq!(report.name, "hash");
        assert!(report.took >= ms(40), "длительность отчёта {:?} меньше фактической", report.took);
        assert!(rx.try_recv().is_err(), "один долгий poll: один отчёт");
    }

    #[tokio::test]
    async fn a_well_behaved_future_is_silent() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        Watched::new("polite", ms(10), async {
            for _ in 0..50 {
                tokio::task::yield_now().await;
            }
        }, tx)
        .await;
        assert!(rx.try_recv().is_err(), "много коротких poll-ов это не тревога");
    }

    #[tokio::test]
    async fn each_poll_is_measured_on_its_own() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        Watched::new("many-small", ms(10), async {
            for _ in 0..5 {
                std::thread::sleep(ms(4));
                tokio::task::yield_now().await;
            }
        }, tx)
        .await;
        assert!(rx.try_recv().is_err(), "пять poll-ов по 4 мс при пороге 10: ни один не долгий, суммировать их нельзя");
    }

    #[tokio::test]
    async fn every_long_poll_gets_a_report() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        Watched::new("twice", ms(10), async {
            std::thread::sleep(ms(20));
            tokio::task::yield_now().await;
            std::thread::sleep(ms(30));
        }, tx)
        .await;
        let first = rx.try_recv().expect("первый долгий poll");
        let second = rx.try_recv().expect("второй долгий poll");
        assert!(first.took >= ms(20) && second.took >= ms(30), "отчёты соответствуют двум poll-ам: {:?} и {:?}", first.took, second.took);
        assert!(rx.try_recv().is_err());
    }
}
