
use std::time::Duration;
use tokio::time::Instant;

fn main() {}

pub struct TokenBucket {
    rate: f64,
    burst: f64,
    tokens: f64,
    last: Instant,
}

impl TokenBucket {
    /// Ведро начинает полным.
    pub fn new(rate_per_sec: f64, burst: u32) -> TokenBucket {
        TokenBucket { rate: rate_per_sec, burst: burst as f64, tokens: burst as f64, last: Instant::now() }
    }

    /// Забирает жетон. Если жетона нет, ничего не списывает и говорит, через сколько он появится.
    pub fn try_acquire(&mut self) -> Result<(), Duration> {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last).as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.rate).min(self.burst);
        self.last = now;
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            Ok(())
        } else {
            Err(Duration::from_secs_f64((1.0 - self.tokens) / self.rate))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: fn(u64) -> Duration = Duration::from_millis;

    fn close(a: Duration, b: Duration) -> bool {
        a.abs_diff(b) <= MS(1)
    }

    #[tokio::test(start_paused = true)]
    async fn a_burst_passes_then_the_bucket_is_empty() {
        let mut b = TokenBucket::new(10.0, 5);
        for i in 0..5 {
            assert!(b.try_acquire().is_ok(), "запрос {i} из всплеска размером 5 должен пройти");
        }
        let wait = b.try_acquire().expect_err("шестой запрос в тот же момент не проходит");
        assert!(close(wait, MS(100)), "до следующего жетона 100 мс (скорость 10 в секунду), а подсказано {wait:?}");
    }

    #[tokio::test(start_paused = true)]
    async fn tokens_drip_at_the_rate() {
        let mut b = TokenBucket::new(10.0, 5);
        for _ in 0..5 {
            b.try_acquire().unwrap();
        }
        tokio::time::advance(MS(100)).await;
        assert!(b.try_acquire().is_ok(), "прошло 100 мс, капнул один жетон");
        assert!(b.try_acquire().is_err(), "а второго нет");
    }

    #[tokio::test(start_paused = true)]
    async fn the_hint_shrinks_as_time_passes() {
        let mut b = TokenBucket::new(10.0, 1);
        b.try_acquire().unwrap();
        tokio::time::advance(MS(30)).await;
        let wait = b.try_acquire().unwrap_err();
        assert!(close(wait, MS(70)), "30 мс из 100 уже прошло, осталось 70 мс, а подсказано {wait:?}");
    }

    #[tokio::test(start_paused = true)]
    async fn the_bucket_never_holds_more_than_burst() {
        let mut b = TokenBucket::new(10.0, 5);
        tokio::time::advance(Duration::from_secs(3600)).await;
        let passed = (0..20).filter(|_| b.try_acquire().is_ok()).count();
        assert_eq!(passed, 5, "после часа простоя в ведре не больше burst жетонов, а прошло {passed}");
    }

    #[tokio::test(start_paused = true)]
    async fn fractions_of_a_token_are_not_lost() {
        let mut b = TokenBucket::new(10.0, 3);
        let mut passed = 0;
        for _ in 0..100 {
            if b.try_acquire().is_ok() {
                passed += 1;
            }
            tokio::time::advance(MS(30)).await;
        }
        assert!((31..=33).contains(&passed), "запросы каждые 30 мс в течение 3 секунд при 10 жетонах в секунду и всплеске 3: ждали около 32, прошло {passed}");
    }

    #[tokio::test(start_paused = true)]
    async fn a_refused_request_costs_nothing() {
        let mut b = TokenBucket::new(10.0, 2);
        b.try_acquire().unwrap();
        b.try_acquire().unwrap();
        for _ in 0..50 {
            assert!(b.try_acquire().is_err());
        }
        tokio::time::advance(MS(100)).await;
        assert!(b.try_acquire().is_ok(), "отказы не должны были ни списывать жетоны, ни откладывать пополнение");
    }
}
