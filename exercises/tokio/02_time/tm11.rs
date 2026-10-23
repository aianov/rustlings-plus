// Ограничитель частоты "ведро с жетонами": пополняется со скоростью rate жетонов в секунду
// до вместимости capacity. В реальной работе всё в порядке, а вот тесты на замороженном
// времени странно падают: ведро упорно не пополняется, сколько бы они ни ждали, и take_wait,
// который должен дождаться жетонов, не дожидается.
// TODO: Найди, откуда берётся время, и почини ведро. Тесты не трогай.
use std::time::Duration;
use std::time::Instant;

struct TokenBucket {
    capacity: f64,
    tokens: f64,
    rate: f64,
    last: Instant,
}

impl TokenBucket {
    fn new(capacity: f64, rate: f64) -> Self {
        TokenBucket { capacity, tokens: capacity, rate, last: Instant::now() }
    }

    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last).as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.rate).min(self.capacity);
        self.last = now;
    }

    fn try_take(&mut self, n: f64) -> bool {
        self.refill();
        if self.tokens >= n {
            self.tokens -= n;
            true
        } else {
            false
        }
    }

    async fn take_wait(&mut self, n: f64) -> bool {
        if self.try_take(n) {
            return true;
        }
        let missing = n - self.tokens;
        tokio::time::sleep(Duration::from_secs_f64(missing / self.rate)).await;
        self.try_take(n)
    }
}

#[tokio::main]
async fn main() {
    let mut bucket = TokenBucket::new(5.0, 1.0);
    println!("{}", bucket.try_take(5.0));
    println!("{}", bucket.take_wait(2.0).await);
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::sleep;

    #[tokio::test(start_paused = true)]
    async fn full_at_start_and_empties() {
        let mut bucket = TokenBucket::new(10.0, 2.0);
        assert!(bucket.try_take(10.0), "ведро стартует полным");
        assert!(!bucket.try_take(1.0), "после опустошения жетонов нет");
    }

    #[tokio::test(start_paused = true)]
    async fn refills_over_time() {
        let mut bucket = TokenBucket::new(10.0, 2.0);
        assert!(bucket.try_take(10.0));
        sleep(Duration::from_secs(3)).await;
        assert!(bucket.try_take(6.0), "за 3 секунды при 2 жетонах в секунду набежало 6: ведро не пополняется");
        assert!(!bucket.try_take(1.0), "больше набежавшего взять нельзя");
    }

    #[tokio::test(start_paused = true)]
    async fn refill_is_capped() {
        let mut bucket = TokenBucket::new(10.0, 2.0);
        assert!(bucket.try_take(10.0));
        sleep(Duration::from_secs(3600)).await;
        assert!(bucket.try_take(10.0), "за час ведро наполнилось до краёв");
        assert!(!bucket.try_take(1.0), "но выше вместимости не бывает");
    }

    #[tokio::test(start_paused = true)]
    async fn take_wait_sleeps_exactly_as_long_as_needed() {
        let mut bucket = TokenBucket::new(4.0, 2.0);
        assert!(bucket.try_take(4.0));
        let started = tokio::time::Instant::now();
        assert!(bucket.take_wait(3.0).await, "take_wait должен дождаться и взять");
        let waited = started.elapsed();
        assert!(
            waited >= Duration::from_millis(1499) && waited <= Duration::from_millis(1501),
            "3 жетона при 2 в секунду копятся 1.5 секунды, а прошло {waited:?}"
        );
    }
}
