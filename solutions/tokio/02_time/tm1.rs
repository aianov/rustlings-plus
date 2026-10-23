use std::time::Duration;
use tokio::time::Instant;

async fn timed_sleep(ms: u64) -> Duration {
    let started = Instant::now();
    tokio::time::sleep(Duration::from_millis(ms)).await;
    started.elapsed()
}

fn backoff(attempt: u32) -> Duration {
    let factor = 1u64.checked_shl(attempt).unwrap_or(u64::MAX);
    Duration::from_millis(100u64.saturating_mul(factor).min(2000))
}

#[tokio::main]
async fn main() {
    println!("{:?}", timed_sleep(50).await);
    println!("{:?}", backoff(3));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn sleep_is_measured_exactly() {
        assert_eq!(timed_sleep(250).await, Duration::from_millis(250), "на замороженных часах замер точный");
    }

    #[tokio::test(start_paused = true)]
    async fn sleep_zero() {
        assert_eq!(timed_sleep(0).await, Duration::ZERO, "нулевой сон не тратит времени");
    }

    #[test]
    fn backoff_doubles() {
        assert_eq!(backoff(0), Duration::from_millis(100), "первая пауза 100 мс");
        assert_eq!(backoff(1), Duration::from_millis(200), "вторая 200 мс");
        assert_eq!(backoff(4), Duration::from_millis(1600), "пятая 1600 мс");
    }

    #[test]
    fn backoff_is_capped() {
        assert_eq!(backoff(5), Duration::from_millis(2000), "3200 мс обрезается до 2000");
        assert_eq!(backoff(40), Duration::from_millis(2000), "большие попытки не переполняются");
        assert_eq!(backoff(1000), Duration::from_millis(2000), "и очень большие тоже");
        assert_eq!(backoff(62), Duration::from_millis(2000), "сдвиг на 62 ещё помещается в u64, но произведение с 100 теряет старшие биты и даёт ноль: умножай насыщающе");
        assert_eq!(backoff(63), Duration::from_millis(2000), "то же на 63: проверь, что проверка переполнения стоит на умножении, а не только на сдвиге");
    }
}
