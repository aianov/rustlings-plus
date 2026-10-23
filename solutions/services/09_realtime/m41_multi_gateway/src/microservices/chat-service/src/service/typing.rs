use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use contracts::{Origin, TypingNote, UserId};

use crate::repository::BoxFuture;

pub const TYPING_PACE: Duration = Duration::from_secs(3);

const PRUNE_AT: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Undelivered;

pub trait Typist: Send + Sync {
    fn typing(&self, note: TypingNote, origin: Origin) -> BoxFuture<'_, Result<(), Undelivered>>;
}

pub struct Offline;

impl Typist for Offline {
    fn typing(&self, _: TypingNote, _: Origin) -> BoxFuture<'_, Result<(), Undelivered>> {
        Box::pin(async { Err(Undelivered) })
    }
}

#[derive(Default)]
pub struct Pace {
    last: Mutex<HashMap<(UserId, UserId), Instant>>,
}

impl Pace {
    fn lock(&self) -> MutexGuard<'_, HashMap<(UserId, UserId), Instant>> {
        self.last.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn allow(&self, from: UserId, to: UserId, now: Instant) -> bool {
        let mut last = self.lock();
        if last.len() >= PRUNE_AT {
            last.retain(|_, at| now.saturating_duration_since(*at) < TYPING_PACE);
        }
        match last.get(&(from, to)) {
            Some(at) if now.saturating_duration_since(*at) < TYPING_PACE => false,
            Some(..) | None => {
                last.insert((from, to), now);
                true
            }
        }
    }
}
