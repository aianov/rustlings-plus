#[cfg(feature = "nats")]
pub mod nats;
#[cfg(feature = "pg")]
pub mod pg;
#[cfg(feature = "redis")]
pub mod redis;
#[cfg(feature = "scylla")]
pub mod scylla;

pub fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

pub fn unique_name(prefix: &str) -> String {
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}_{}_{nanos:x}_{n}", std::process::id())
}
