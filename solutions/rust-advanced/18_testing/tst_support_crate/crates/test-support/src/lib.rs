use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(feature = "test-util")]
mod fakes;

#[cfg(feature = "test-util")]
pub use fakes::{FakeClock, SeqIds};

pub fn unique_name(prefix: &str) -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    format!("{prefix}_{}_{}", std::process::id(), COUNTER.fetch_add(1, Ordering::SeqCst))
}
