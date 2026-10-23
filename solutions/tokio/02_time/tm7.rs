use std::time::Duration;
use tokio::time::{Instant, interval};

async fn work() {
    tokio::time::sleep(Duration::from_millis(30)).await;
}

async fn run_rounds(rounds: usize) -> Vec<u64> {
    let started = Instant::now();
    let mut ticker = interval(Duration::from_millis(100));
    let mut starts = Vec::new();
    for _ in 0..rounds {
        ticker.tick().await;
        starts.push(started.elapsed().as_millis() as u64);
        work().await;
    }
    starts
}

#[tokio::main]
async fn main() {
    println!("{:?}", run_rounds(4).await);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn rounds_start_on_the_grid() {
        assert_eq!(
            run_rounds(5).await,
            [0, 100, 200, 300, 400],
            "каждый раунд должен стартовать по сетке в 100 мс. Пауза после работы копит дрейф: 30 мс на каждый раунд"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn single_round() {
        assert_eq!(run_rounds(1).await, [0], "первый раунд стартует сразу");
    }

    #[tokio::test(start_paused = true)]
    async fn no_rounds() {
        assert!(run_rounds(0).await.is_empty(), "ноль раундов: пустой ответ");
    }
}
