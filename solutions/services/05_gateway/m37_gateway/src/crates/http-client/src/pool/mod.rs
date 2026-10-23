use std::collections::HashMap;
use std::time::Duration;

use tokio::time::Instant;

pub type Key = (bool, String, u16);

pub struct Pool<S> {
    max_idle: usize,
    ttl: Duration,
    idle: HashMap<Key, Vec<(Instant, S)>>,
}

impl<S> Pool<S> {
    pub fn new(max_idle: usize, ttl: Duration) -> Pool<S> {
        Pool {
            max_idle,
            ttl,
            idle: HashMap::new(),
        }
    }

    pub fn take(&mut self, key: &Key) -> Option<S> {
        let list = self.idle.get_mut(key)?;
        let (now, ttl) = (Instant::now(), self.ttl);
        list.retain(|(since, _)| now.duration_since(*since) < ttl);
        list.pop().map(|(_, s)| s)
    }

    pub fn put(&mut self, key: Key, conn: S) {
        let list = self.idle.entry(key).or_default();
        list.push((Instant::now(), conn));
        while list.len() > self.max_idle {
            list.remove(0);
        }
    }

    pub fn idle(&self) -> usize {
        self.idle.values().map(Vec::len).sum()
    }
}
