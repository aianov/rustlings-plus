use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug)]
pub struct SeqIds {
    next: AtomicU64,
}

impl SeqIds {
    pub fn starting_at(first: u64) -> SeqIds {
        SeqIds {
            next: AtomicU64::new(first),
        }
    }

    pub fn next_id(&self) -> u64 {
        self.next.fetch_add(1, Ordering::Relaxed)
    }
}
