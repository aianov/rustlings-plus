
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;
use tokio::time::Instant;

fn main() {}

pub struct Cache {
    ttl: Duration,
    entries: Mutex<HashMap<String, (String, Instant)>>,
}

fn ttl_for(base: Duration, key: &str) -> Duration {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in key.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x100000001b3);
    }
    h ^= h >> 33;
    h = h.wrapping_mul(0xff51afd7ed558ccd);
    h ^= h >> 33;
    let spread = (h >> 11) as f64 / (1u64 << 53) as f64;
    base.mul_f64(0.9 + 0.2 * spread)
}

impl Cache {
    pub fn new(ttl: Duration) -> Cache {
        Cache { ttl, entries: Mutex::new(HashMap::new()) }
    }

    pub fn put(&self, key: &str, value: &str) {
        let expires = Instant::now() + ttl_for(self.ttl, key);
        self.entries.lock().unwrap().insert(key.to_string(), (value.to_string(), expires));
    }

    pub fn get(&self, key: &str) -> Option<String> {
        let entries = self.entries.lock().unwrap();
        entries.get(key).filter(|(_, expires)| Instant::now() < *expires).map(|(v, _)| v.clone())
    }

    /// Через сколько протухнет запись.
    pub fn expires_in(&self, key: &str) -> Option<Duration> {
        let entries = self.entries.lock().unwrap();
        entries.get(key).map(|(_, expires)| expires.saturating_duration_since(Instant::now()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOUR: Duration = Duration::from_secs(3600);

    fn warm() -> Cache {
        let cache = Cache::new(HOUR);
        for i in 0..1000 {
            cache.put(&format!("user:{i}"), "профиль");
        }
        cache
    }

    #[tokio::test(start_paused = true)]
    async fn ttl_stays_within_ten_percent_of_the_base() {
        let cache = warm();
        for i in 0..1000 {
            let left = cache.expires_in(&format!("user:{i}")).unwrap();
            assert!(left >= HOUR.mul_f64(0.9) && left <= HOUR.mul_f64(1.1), "срок {left:?} вышел за границы 90..110% от часа");
        }
    }

    #[tokio::test(start_paused = true)]
    async fn expiries_are_spread_evenly() {
        let cache = warm();
        let mut buckets = [0u32; 20];
        for i in 0..1000 {
            let left = cache.expires_in(&format!("user:{i}")).unwrap().as_secs_f64();
            let slot = (((left / 3600.0) - 0.9) / 0.2 * 20.0).clamp(0.0, 19.0) as usize;
            buckets[slot] += 1;
        }
        let worst = *buckets.iter().max().unwrap();
        assert!(worst <= 100, "в одном из двадцати окон протухает {worst} из 1000 записей: срок почти одинаков, будильник на месте ({buckets:?})");
        assert!(buckets.iter().filter(|n| **n > 0).count() >= 18, "записи занимают не всё окно: {buckets:?}");
    }

    #[tokio::test(start_paused = true)]
    async fn one_key_gets_the_same_ttl_every_time() {
        let cache = Cache::new(HOUR);
        cache.put("user:7", "а");
        let first = cache.expires_in("user:7").unwrap();
        cache.put("user:7", "б");
        assert_eq!(cache.expires_in("user:7").unwrap(), first, "для одного ключа срок должен быть воспроизводим");
    }

    #[tokio::test(start_paused = true)]
    async fn entries_still_expire() {
        let cache = warm();
        tokio::time::advance(HOUR.mul_f64(0.89)).await;
        assert!(cache.get("user:1").is_some());
        tokio::time::advance(HOUR.mul_f64(0.22)).await;
        assert!((0..1000).all(|i| cache.get(&format!("user:{i}")).is_none()), "через 111% базового срока не должно остаться живых записей");
    }
}
