use async_nats::HeaderMap;
use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub const REQUEST_ID: &str = "X-Request-Id";
pub const TRACE_ID: &str = "X-Trace-Id";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Meta {
    pub request_id: String,
    pub trace_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetaError(pub String);

fn good_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= 64 && value.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn random_hex() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let mut hasher = RandomState::new().build_hasher();
    hasher.write_u64(COUNTER.fetch_add(1, Ordering::Relaxed));
    hasher.write_u128(nanos);
    format!("{:016x}", hasher.finish())
}

impl Meta {
    pub fn new(request_id: &str, trace_id: &str) -> Result<Meta, MetaError> {
        todo!()
    }

    pub fn fresh() -> Meta {
        todo!()
    }

    pub(crate) fn to_headers(&self) -> HeaderMap {
        todo!()
    }

    pub(crate) fn from_headers(headers: Option<&HeaderMap>) -> Option<Meta> {
        todo!()
    }
}
