use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Session {
    pub hits: u64,
}

pub struct SessionStore {
    all: Mutex<HashMap<u64, Session>>,
}

impl SessionStore {
    pub fn new(shards: usize) -> SessionStore {
        let _ = shards;
        SessionStore { all: Mutex::new(HashMap::new()) }
    }

    pub fn shard_of(&self, user: u64) -> usize {
        let _ = user;
        0
    }

    pub fn with<R>(&self, user: u64, f: impl FnOnce(&mut Session) -> R) -> R {
        let mut all = self.all.lock().expect("сессии не отравлены");
        f(all.entry(user).or_default())
    }

    pub fn total_hits(&self) -> u64 {
        self.all.lock().expect("сессии не отравлены").values().map(|v| v.hits).sum()
    }
}
