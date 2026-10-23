use std::collections::HashMap;
use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use contracts::{UserId, UserView};
use service_runtime::Lru;
use tokio::sync::watch;
use tokio::time::Instant;

pub const PROFILE_CAPACITY: usize = 10_000;

pub const MISS_FOR: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Known {
    Found(UserView),
    Missing,
}

#[derive(Debug, Clone)]
struct Entry {
    known: Known,
    until: Instant,
}

struct Inner {
    lru: Lru<UserId, Entry>,
    flights: HashMap<UserId, watch::Receiver<bool>>,
    epochs: HashMap<UserId, u64>,
    versions: HashMap<UserId, u64>,
}

pub struct Flight {
    pub id: UserId,
    epoch: u64,
    done: watch::Sender<bool>,
}

pub enum Claim {
    Mine(Flight),
    Wait(watch::Receiver<bool>),
}

pub struct ProfileCache {
    fresh_for: Duration,
    miss_for: Duration,
    inner: Mutex<Inner>,
}

fn jitter(base: Duration) -> Duration {
    let mut hasher = RandomState::new().build_hasher();
    hasher.write_u8(1);
    let spread = (hasher.finish() % 2001) as f64 / 10_000.0;
    base.mul_f64(0.9 + spread)
}

impl ProfileCache {
    pub fn new(fresh_for: Duration) -> ProfileCache {
        ProfileCache::sized(fresh_for, PROFILE_CAPACITY)
    }

    pub fn sized(fresh_for: Duration, capacity: usize) -> ProfileCache {
        ProfileCache {
            fresh_for,
            miss_for: MISS_FOR.min(fresh_for.max(Duration::from_millis(1))),
            inner: Mutex::new(Inner {
                lru: Lru::new(capacity),
                flights: HashMap::new(),
                epochs: HashMap::new(),
                versions: HashMap::new(),
            }),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn fresh(&self, id: UserId) -> Option<Known> {
        let mut inner = self.lock();
        let entry = inner.lru.get(&id)?;
        (Instant::now() < entry.until).then(|| entry.known.clone())
    }

    pub fn stale(&self, id: UserId) -> Option<UserView> {
        match &self.lock().lru.peek(&id)?.known {
            Known::Found(view) => Some(view.clone()),
            Known::Missing => None,
        }
    }

    pub fn put(&self, view: UserView) {
        let until = Instant::now() + jitter(self.fresh_for);
        let id = view.id;
        self.lock().lru.put(
            id,
            Entry {
                known: Known::Found(view),
                until,
            },
        );
    }

    pub fn claim(&self, id: UserId) -> Claim {
        let mut inner = self.lock();
        if let Some(waiting) = inner.flights.get(&id) {
            return Claim::Wait(waiting.clone());
        }
        let (done, waiting) = watch::channel(false);
        inner.flights.insert(id, waiting);
        let epoch = inner.epochs.get(&id).copied().unwrap_or(0);
        Claim::Mine(Flight { id, epoch, done })
    }

    pub fn finish(&self, flight: Flight, known: Option<Known>) {
        let mut inner = self.lock();
        inner.flights.remove(&flight.id);
        let same = inner.epochs.get(&flight.id).copied().unwrap_or(0) == flight.epoch;
        if let (Some(known), true) = (known, same) {
            let ttl = match known {
                Known::Found(..) => jitter(self.fresh_for),
                Known::Missing => jitter(self.miss_for),
            };
            inner.lru.put(
                flight.id,
                Entry {
                    known,
                    until: Instant::now() + ttl,
                },
            );
        }
        drop(inner);
        let _ = flight.done.send(true);
    }

    pub fn invalidate(&self, id: UserId, version: u64) -> bool {
        let mut inner = self.lock();
        let seen = inner.versions.get(&id).copied().unwrap_or(0);
        if version <= seen {
            return false;
        }
        inner.versions.insert(id, version);
        *inner.epochs.entry(id).or_default() += 1;
        inner.lru.remove(&id);
        true
    }

    pub fn len(&self) -> usize {
        self.lock().lru.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
