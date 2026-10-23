use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static SERIAL: AtomicU64 = AtomicU64::new(0);

pub fn unique_root(prefix: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.subsec_nanos());
    let serial = SERIAL.fetch_add(1, Ordering::Relaxed);
    let prefix: String = prefix
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(8)
        .collect();
    format!(
        "{prefix}{}_{nanos:x}_{serial}",
        std::process::id() % 100_000
    )
}
