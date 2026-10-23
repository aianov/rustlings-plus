// Нужен тикер как поток: раз в период он выдаёт текущий момент, всего `left` раз, потом
// заканчивается. Готовый `tokio::time::Interval` умеет «ждать следующий тик» только как
// future, а потоком он не является. Свяжи их: ждать в `poll_next` через `.await` нельзя, это
// обычная функция.
// TODO: Реализуй `Stream` для `Ticker`.
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

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let _ = (cx, self.left);
        todo!()
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
