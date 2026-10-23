use std::collections::HashMap;
use std::hash::Hash;
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use tokio::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TokenBucket {
    pub tokens: f64,
    pub last: Instant,
}

impl TokenBucket {
    pub fn full(burst: f64, now: Instant) -> TokenBucket {
        TokenBucket {
            tokens: burst,
            last: now,
        }
    }

    pub fn refill(&mut self, rate: f64, burst: f64, now: Instant) {
        let spent = now.saturating_duration_since(self.last).as_secs_f64();
        self.tokens = (self.tokens + spent * rate).min(burst);
        self.last = now;
    }

    pub fn take(&mut self, rate: f64, burst: f64, now: Instant) -> Result<(), Duration> {
        self.refill(rate, burst, now);
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            return Ok(());
        }
        let missing = 1.0 - self.tokens;
        Err(Duration::from_secs_f64(
            missing / rate.max(f64::MIN_POSITIVE),
        ))
    }
}

#[derive(Debug)]
pub struct Buckets<K> {
    rate: f64,
    burst: f64,
    map: Mutex<HashMap<K, TokenBucket>>,
}

impl<K: Eq + Hash + Clone> Buckets<K> {
    pub fn new(rate: f64, burst: f64) -> Buckets<K> {
        Buckets {
            rate,
            burst: burst.max(1.0),
            map: Mutex::new(HashMap::new()),
        }
    }

    pub fn take(&self, key: &K) -> Result<(), Duration> {
        self.take_at(key, Instant::now())
    }

    pub fn take_at(&self, key: &K, now: Instant) -> Result<(), Duration> {
        let mut map = self.map.lock().unwrap_or_else(PoisonError::into_inner);
        let bucket = map
            .entry(key.clone())
            .or_insert_with(|| TokenBucket::full(self.burst, now));
        bucket.take(self.rate, self.burst, now)
    }

    pub fn evict_idle(&self, idle: Duration, now: Instant) -> usize {
        let mut map = self.map.lock().unwrap_or_else(PoisonError::into_inner);
        let before = map.len();
        let (rate, burst) = (self.rate, self.burst);
        map.retain(|_, bucket| {
            let mut probe = *bucket;
            probe.refill(rate, burst, now);
            !(probe.tokens >= burst && now.saturating_duration_since(bucket.last) >= idle)
        });
        before - map.len()
    }

    pub fn len(&self) -> usize {
        self.map
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
