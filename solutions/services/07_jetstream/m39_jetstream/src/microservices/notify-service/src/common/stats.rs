use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug, Default)]
pub struct Stats {
    pub received: AtomicUsize,
    pub ignored: AtomicUsize,
    pub rejected: AtomicUsize,
    pub retried: AtomicUsize,
    pub handled: AtomicUsize,
    pub stored: AtomicUsize,
    pub duplicates: AtomicUsize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatsSnapshot {
    pub received: usize,
    pub ignored: usize,
    pub rejected: usize,
    pub retried: usize,
    pub handled: usize,
    pub stored: usize,
    pub duplicates: usize,
}

impl Stats {
    pub fn bump(counter: &AtomicUsize) {
        Stats::add(counter, 1);
    }

    pub fn add(counter: &AtomicUsize, amount: usize) {
        counter.fetch_add(amount, Ordering::SeqCst);
    }

    pub fn snapshot(&self) -> StatsSnapshot {
        let get = |counter: &AtomicUsize| counter.load(Ordering::SeqCst);
        StatsSnapshot {
            received: get(&self.received),
            ignored: get(&self.ignored),
            rejected: get(&self.rejected),
            retried: get(&self.retried),
            handled: get(&self.handled),
            stored: get(&self.stored),
            duplicates: get(&self.duplicates),
        }
    }
}

impl StatsSnapshot {
    pub fn balanced(&self) -> bool {
        self.received == self.ignored + self.rejected + self.retried + self.handled
    }
}
