use futures::Stream;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::time::{interval, Instant, Interval};

struct Ticker {
    interval: Interval,
    left: u32,
}

impl Ticker {
    fn new(period: Duration, count: u32) -> Ticker {
        Ticker { interval: interval(period), left: count }
    }
}

impl Stream for Ticker {
    type Item = Instant;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.left == 0 {
            return Poll::Ready(None);
        }
        match self.interval.poll_tick(cx) {
            Poll::Ready(at) => {
                self.left -= 1;
                Poll::Ready(Some(at))
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

#[tokio::main]
async fn main() {
    use futures::StreamExt;
    let ticks: Vec<_> = Ticker::new(Duration::from_millis(20), 3).collect().await;
    println!("{}", ticks.len());
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;

    #[tokio::test(start_paused = true)]
    async fn ticks_at_the_period() {
        let start = Instant::now();
        let ticks: Vec<Duration> = Ticker::new(Duration::from_millis(100), 3).map(|at| at - start).collect().await;
        let expected = vec![Duration::ZERO, Duration::from_millis(100), Duration::from_millis(200)];
        assert_eq!(ticks, expected, "первый тик сразу, потом каждые 100 мс, всего три");
    }

    #[tokio::test(start_paused = true)]
    async fn stops_after_count_without_waiting() {
        let start = Instant::now();
        let mut ticker = Ticker::new(Duration::from_secs(10), 1);
        assert!(ticker.next().await.is_some(), "один тик есть");
        assert!(ticker.next().await.is_none(), "после последнего тика поток кончился");
        assert!(start.elapsed() < Duration::from_secs(1), "конец потока не должен ждать лишний период");
    }

    #[tokio::test(start_paused = true)]
    async fn zero_ticks() {
        assert!(Ticker::new(Duration::from_millis(10), 0).next().await.is_none(), "ноль тиков: поток пуст");
    }
}
