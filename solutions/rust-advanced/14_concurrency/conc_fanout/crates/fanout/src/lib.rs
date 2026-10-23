//! Фоновая рассылка уведомлений.

mod pool;

use pool::Pool;
use std::collections::{BTreeSet, HashMap};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};

pub type UserId = u32;

pub trait ProfileStore: Send + Sync {
    fn load(&self, id: UserId) -> Option<String>;
}

pub trait Sink: Send + Sync {
    fn deliver(&self, to: UserId, text: &str) -> Result<(), String>;
}

#[derive(Debug, PartialEq)]
pub enum RegisterError {
    Invalid,
    Taken,
}

#[derive(Debug, PartialEq)]
pub enum FollowError {
    SameUser,
    UnknownUser(UserId),
}

#[derive(Debug, PartialEq)]
pub enum PublishError {
    UnknownUser,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Stats {
    pub posts: u64,
    pub queued: u64,
    pub delivered: u64,
    pub failed: u64,
}

struct User {
    login: String,
    friends: Mutex<BTreeSet<UserId>>,
}

#[derive(Default)]
struct Registry {
    by_login: HashMap<String, UserId>,
    users: HashMap<UserId, Arc<User>>,
}

#[derive(Default)]
struct Counters {
    posts: AtomicU64,
    queued: AtomicU64,
    delivered: AtomicU64,
    failed: AtomicU64,
}

pub struct Fanout {
    registry: RwLock<Registry>,
    profiles: RwLock<HashMap<UserId, Arc<String>>>,
    store: Arc<dyn ProfileStore>,
    sink: Arc<dyn Sink>,
    counters: Arc<Counters>,
    pool: Pool,
}

fn valid_login(login: &str) -> bool {
    !login.is_empty() && login.len() <= 20 && login.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

impl Fanout {
    pub fn new(workers: usize, store: Arc<dyn ProfileStore>, sink: Arc<dyn Sink>) -> Fanout {
        Fanout {
            registry: RwLock::new(Registry::default()),
            profiles: RwLock::new(HashMap::new()),
            store,
            sink,
            counters: Arc::new(Counters::default()),
            pool: Pool::new(workers),
        }
    }

    fn user(&self, id: UserId) -> Option<Arc<User>> {
        self.registry.read().unwrap().users.get(&id).cloned()
    }

    pub fn register(&self, login: &str) -> Result<UserId, RegisterError> {
        if !valid_login(login) {
            return Err(RegisterError::Invalid);
        }
        let mut registry = self.registry.write().unwrap();
        if registry.by_login.contains_key(login) {
            return Err(RegisterError::Taken);
        }
        let id = registry.users.len() as UserId + 1;
        registry.by_login.insert(login.to_string(), id);
        registry.users.insert(id, Arc::new(User { login: login.to_string(), friends: Mutex::new(BTreeSet::new()) }));
        Ok(id)
    }

    pub fn profile(&self, id: UserId) -> Option<Arc<String>> {
        if let Some(found) = self.profiles.read().unwrap().get(&id) {
            return Some(Arc::clone(found));
        }
        let loaded = Arc::new(self.store.load(id)?);
        let mut cache = self.profiles.write().unwrap();
        Some(Arc::clone(cache.entry(id).or_insert(loaded)))
    }

    pub fn befriend(&self, a: UserId, b: UserId) -> Result<(), FollowError> {
        if a == b {
            return Err(FollowError::SameUser);
        }
        let user_a = self.user(a).ok_or(FollowError::UnknownUser(a))?;
        let user_b = self.user(b).ok_or(FollowError::UnknownUser(b))?;
        let ((low_id, low), (high_id, high)) = if a < b { ((a, &user_a), (b, &user_b)) } else { ((b, &user_b), (a, &user_a)) };
        let mut low_friends = low.friends.lock().unwrap();
        let mut high_friends = high.friends.lock().unwrap();
        low_friends.insert(high_id);
        high_friends.insert(low_id);
        Ok(())
    }

    pub fn friends(&self, id: UserId) -> Vec<UserId> {
        match self.user(id) {
            Some(user) => user.friends.lock().unwrap().iter().copied().collect(),
            None => Vec::new(),
        }
    }

    pub fn publish(&self, author: UserId, text: &str) -> Result<usize, PublishError> {
        let author = self.user(author).ok_or(PublishError::UnknownUser)?;
        if self.pool.is_closed() {
            return Err(PublishError::Closed);
        }
        let recipients: Vec<UserId> = author.friends.lock().unwrap().iter().copied().collect();
        self.counters.posts.fetch_add(1, Ordering::Relaxed);
        let message = format!("{} опубликовал: {}", author.login, text);
        let mut queued = 0;
        for to in recipients {
            let sink = Arc::clone(&self.sink);
            let counters = Arc::clone(&self.counters);
            let message = message.clone();
            self.counters.queued.fetch_add(1, Ordering::Relaxed);
            let accepted = self.pool.execute(move || match sink.deliver(to, &message) {
                Ok(()) => {
                    counters.delivered.fetch_add(1, Ordering::Relaxed);
                }
                Err(_) => {
                    counters.failed.fetch_add(1, Ordering::Relaxed);
                }
            });
            if accepted.is_err() {
                self.counters.queued.fetch_sub(1, Ordering::Relaxed);
                return Err(PublishError::Closed);
            }
            queued += 1;
        }
        Ok(queued)
    }

    pub fn stats(&self) -> Stats {
        Stats {
            posts: self.counters.posts.load(Ordering::Relaxed),
            queued: self.counters.queued.load(Ordering::Relaxed),
            delivered: self.counters.delivered.load(Ordering::Relaxed),
            failed: self.counters.failed.load(Ordering::Relaxed),
        }
    }

    pub fn shutdown(&self) -> Stats {
        self.pool.close();
        self.pool.wait_idle();
        self.stats()
    }
}
