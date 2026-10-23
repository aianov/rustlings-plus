use std::collections::HashMap;
use std::hash::{BuildHasher, Hash, RandomState};
use std::sync::{Mutex, MutexGuard, PoisonError};

pub const DEFAULT_SHARDS: usize = 16;

pub struct Shards<K, V> {
    shards: Vec<Mutex<HashMap<K, V>>>,
    hasher: RandomState,
}

impl<K: Hash + Eq, V> Shards<K, V> {
    pub fn new(count: usize) -> Shards<K, V> {
        Shards {
            shards: (0..count.max(1))
                .map(|_| Mutex::new(HashMap::new()))
                .collect(),
            hasher: RandomState::new(),
        }
    }

    pub fn count(&self) -> usize {
        self.shards.len()
    }

    pub fn shard_of(&self, key: &K) -> usize {
        let spread = self.hasher.hash_one(key) % self.shards.len() as u64;
        spread as usize
    }

    pub fn with<R>(&self, key: &K, f: impl FnOnce(&mut HashMap<K, V>) -> R) -> R {
        f(&mut self.lock(self.shard_of(key)))
    }

    pub fn get(&self, key: &K) -> Option<V>
    where
        V: Clone,
    {
        self.with(key, |shard| shard.get(key).cloned())
    }

    pub fn len(&self) -> usize {
        (0..self.count()).map(|index| self.lock(index).len()).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn lock(&self, index: usize) -> MutexGuard<'_, HashMap<K, V>> {
        self.shards[index]
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

impl<K: Hash + Eq, V> Default for Shards<K, V> {
    fn default() -> Shards<K, V> {
        Shards::new(DEFAULT_SHARDS)
    }
}
