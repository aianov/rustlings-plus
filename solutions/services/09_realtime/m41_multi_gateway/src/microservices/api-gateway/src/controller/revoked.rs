use std::collections::{HashSet, VecDeque};
use std::sync::{Mutex, MutexGuard, PoisonError};

use contracts::MAX_REVOKED;

#[derive(Default)]
struct Memory {
    order: VecDeque<String>,
    known: HashSet<String>,
}

#[derive(Default)]
pub(super) struct Revoked {
    memory: Mutex<Memory>,
}

impl Revoked {
    fn lock(&self) -> MutexGuard<'_, Memory> {
        self.memory.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(super) fn remember(&self, sessions: &[String]) -> HashSet<String> {
        let mut memory = self.lock();
        let mut fresh = HashSet::new();
        for session in sessions.iter().filter(|session| !session.is_empty()) {
            fresh.insert(session.clone());
            if memory.known.insert(session.clone()) {
                memory.order.push_back(session.clone());
            }
        }
        while memory.order.len() > MAX_REVOKED {
            if let Some(oldest) = memory.order.pop_front() {
                memory.known.remove(&oldest);
            }
        }
        fresh
    }

    pub(super) fn contains(&self, session: &str) -> bool {
        self.lock().known.contains(session)
    }

    pub(super) fn len(&self) -> usize {
        self.lock().order.len()
    }
}
