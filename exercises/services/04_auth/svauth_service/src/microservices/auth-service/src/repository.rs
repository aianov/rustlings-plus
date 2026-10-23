use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

#[derive(Debug, Clone)]
pub struct User {
    pub id: String,
    pub username: String,
    pub email: String,
    pub password_hash: String,
    pub role: String,
}

#[derive(Debug, Clone)]
pub struct SessionRow {
    pub family: u64,
    pub user_id: String,
    pub used: bool,
    pub expires: u64,
    pub access_jti: String,
    pub access_exp: u64,
}

#[derive(Default)]
pub struct Inner {
    pub users: HashMap<String, User>,
    pub by_name: HashMap<String, String>,
    pub sessions: HashMap<String, SessionRow>,
    pub revoked_families: HashSet<u64>,
    pub denied: HashMap<String, u64>,
    pub next_user: u64,
    pub next_family: u64,
}

#[derive(Default)]
pub struct Store {
    inner: Mutex<Inner>,
}

impl Store {
    pub fn new() -> Store {
        Store::default()
    }

    pub fn transaction<R>(&self, work: impl FnOnce(&mut Inner) -> R) -> R {
        let mut inner = self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        work(&mut inner)
    }

    pub fn password_hashes(&self) -> Vec<String> {
        self.transaction(|db| db.users.values().map(|u| u.password_hash.clone()).collect())
    }

    pub fn session_keys(&self) -> Vec<String> {
        self.transaction(|db| db.sessions.keys().cloned().collect())
    }
}
