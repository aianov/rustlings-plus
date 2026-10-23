use std::time::Duration;
use tokio::time::Instant;
use tokio::time::interval_at;

async fn poll_times(period_ms: u64, count: usize) -> Vec<u64> {
    let started = Instant::now();
    let period = Duration::from_millis(period_ms);
    let mut ticker = interval_at(started + period, period);
    let mut times = Vec::new();
    for _ in 0..count {
        ticker.tick().await;
        times.push(started.elapsed().as_millis() as u64);
    }
    times
}

#[tokio::main]
async fn main() {
    println!("{:?}", poll_times(50, 3).await);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn first_poll_is_after_one_period() {
        assert_eq!(
            poll_times(50, 4).await,
            [50, 100, 150, 200],
            "первый тик interval приходит сразу, а опрос должен ждать целый период"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn other_period() {
        assert_eq!(poll_times(120, 2).await, [120, 240], "период берётся из аргумента");
    }

    #[tokio::test(start_paused = true)]
    async fn nothing_to_poll() {
        assert!(poll_times(50, 0).await.is_empty(), "ноль опросов: пустой ответ");
    }
}
