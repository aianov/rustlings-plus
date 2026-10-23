// Лимитер по IP-адресу работает, а память шлюза растёт сутками: на каждый адрес, который хоть раз постучался, заводится ведро, и
// оно не удаляется никогда. Адресов в интернете много, ботам достаточно по одному запросу с каждого, чтобы за неделю съесть
// гигабайты. Но выбрасывать ведро нужно с умом: полностью наполнившееся ведро неотличимо от нового, а вот только что опустошённое
// забывать нельзя, иначе нарушитель получает свежий всплеск, как только его ведро «убрали».
// Цель: размер карты ограничен числом недавно активных адресов, а лимит для нарушителей не ослабляется.


use std::collections::HashMap;
use std::time::{Duration, Instant};

fn main() {}

struct Bucket {
    tokens: f64,
    last: Instant,
}

pub struct Limiter {
    rate: f64,
    burst: f64,
    buckets: HashMap<u32, Bucket>,
}

impl Limiter {
    pub fn new(rate_per_sec: f64, burst: u32) -> Limiter {
        Limiter { rate: rate_per_sec, burst: burst as f64, buckets: HashMap::new() }
    }

    /// Сколько адресов хранится сейчас.
    pub fn tracked(&self) -> usize {
        self.buckets.len()
    }

    pub fn allow(&mut self, ip: u32, now: Instant) -> bool {
        let bucket = self.buckets.entry(ip).or_insert(Bucket { tokens: self.burst, last: now });
        let elapsed = now.saturating_duration_since(bucket.last).as_secs_f64();
        bucket.tokens = (bucket.tokens + elapsed * self.rate).min(self.burst);
        bucket.last = now;
        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: fn(u64) -> Duration = Duration::from_millis;

    #[test]
    fn still_limits_correctly() {
        let t0 = Instant::now();
        let mut l = Limiter::new(1.0, 3);
        assert!((0..3).all(|_| l.allow(1, t0)), "всплеск из трёх проходит");
        assert!(!l.allow(1, t0), "четвёртый запрос с того же адреса в тот же момент не проходит");
        assert!(l.allow(2, t0), "другой адрес имеет своё ведро");
        assert!(l.allow(1, t0 + MS(1000)), "через секунду капнул жетон");
    }

    #[test]
    fn memory_is_bounded_by_recently_active_addresses() {
        let wall = Instant::now();
        let t0 = Instant::now();
        let mut l = Limiter::new(1.0, 5);
        for i in 0..100_000u32 {
            l.allow(i, t0 + MS(u64::from(i)));
        }
        assert!(l.tracked() <= 12_000, "после 100 000 разных адресов за 100 секунд хранится {}: вёдра выброшенных давно адресов остались", l.tracked());
        assert!(wall.elapsed() < Duration::from_secs(10), "очистка стоит слишком дорого: полный обход карты на каждом запросе");
        l.allow(999_999, t0 + Duration::from_secs(200));
        assert!(l.tracked() <= 2, "после долгой тишины хранится {} адресов, а должен остаться только новый", l.tracked());
    }

    #[test]
    fn a_drained_bucket_is_not_forgotten() {
        let t0 = Instant::now();
        let mut l = Limiter::new(0.01, 5);
        assert!((0..5).all(|_| l.allow(7, t0)));
        for i in 0..30_000u32 {
            l.allow(100 + i, t0 + MS(u64::from(i) / 2));
        }
        assert!(!l.allow(7, t0 + Duration::from_secs(15)), "нарушитель опустошил ведро, прошло 15 секунд при 0.01 жетона в секунду: жетона ещё нет, а лимитер его уже забыл");
    }

    #[test]
    fn a_refilled_bucket_may_go() {
        let t0 = Instant::now();
        let mut l = Limiter::new(1.0, 2);
        l.allow(1, t0);
        l.allow(2, t0);
        l.allow(3, t0 + Duration::from_secs(10));
        l.allow(4, t0 + Duration::from_secs(20));
        assert!(l.tracked() <= 2, "давно полные вёдра первых адресов должны были уйти, хранится {}", l.tracked());
    }
}
