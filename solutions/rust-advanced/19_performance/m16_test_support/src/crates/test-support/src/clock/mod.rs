use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use contracts::{Clock, Timestamp};

#[derive(Debug, Clone, Default)]
pub struct FakeClock {
    now: Arc<AtomicU64>,
}

impl FakeClock {
    pub fn new(start: Timestamp) -> FakeClock {
        FakeClock {
            now: Arc::new(AtomicU64::new(start)),
        }
    }

    pub fn advance(&self, secs: u64) {
        self.now.fetch_add(secs, Ordering::SeqCst);
    }

    pub fn set(&self, at: Timestamp) {
        self.now.store(at, Ordering::SeqCst);
    }
}

impl Clock for FakeClock {
    fn now(&self) -> Timestamp {
        self.now.load(Ordering::SeqCst)
    }
}
