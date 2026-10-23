use contracts::{Clock, Ids};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone)]
pub struct FakeClock {
    now: Arc<AtomicU64>,
}

impl FakeClock {
    pub fn new(now: u64) -> FakeClock {
        FakeClock { now: Arc::new(AtomicU64::new(now)) }
    }

    pub fn advance(&self, secs: u64) {
        self.now.fetch_add(secs, Ordering::SeqCst);
    }
}

impl Clock for FakeClock {
    fn now(&self) -> u64 {
        self.now.load(Ordering::SeqCst)
    }
}

pub struct SeqIds {
    next: AtomicU64,
}

impl SeqIds {
    pub fn starting_at(first: u64) -> SeqIds {
        SeqIds { next: AtomicU64::new(first) }
    }
}

impl Ids for SeqIds {
    fn next_id(&self) -> u64 {
        self.next.fetch_add(1, Ordering::SeqCst)
    }
}
