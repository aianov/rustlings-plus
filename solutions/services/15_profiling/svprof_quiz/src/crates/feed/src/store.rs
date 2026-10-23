use std::collections::HashMap;
use std::hash::{BuildHasher, BuildHasherDefault, DefaultHasher};
use std::sync::Mutex;

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Session {
    pub hits: u64,
}

pub struct SessionStore {
    shards: Vec<Mutex<HashMap<u64, Session>>>,
}

impl SessionStore {
    pub fn new(shards: usize) -> SessionStore {
        assert!(shards > 0, "нужен хотя бы один шард");
        SessionStore { shards: (0..shards).map(|_| Mutex::new(HashMap::new())).collect() }
    }

    pub fn shard_of(&self, user: u64) -> usize {
        (BuildHasherDefault::<DefaultHasher>::default().hash_one(user) % self.shards.len() as u64) as usize
    }

    pub fn with<R>(&self, user: u64, f: impl FnOnce(&mut Session) -> R) -> R {
        let mut shard = self.shards[self.shard_of(user)].lock().expect("шард не отравлен");
        f(shard.entry(user).or_default())
    }

    pub fn total_hits(&self) -> u64 {
        self.shards.iter().map(|s| s.lock().expect("шард не отравлен").values().map(|v| v.hits).sum::<u64>()).sum()
    }
}
