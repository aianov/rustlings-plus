use std::time::{SystemTime, UNIX_EPOCH};

use super::Timestamp;

pub fn now() -> Timestamp {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or_default()
}
