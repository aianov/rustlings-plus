use std::collections::hash_map::RandomState;
use std::error::Error;
use std::fmt;
use std::hash::{BuildHasher, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use async_nats::HeaderMap;

pub const REQUEST_ID: &str = "X-Request-Id";

pub const TRACE_ID: &str = "X-Trace-Id";

const ID_MAX: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Meta {
    pub request_id: String,
    pub trace_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetaError(pub String);

impl fmt::Display for MetaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Error for MetaError {}

pub fn good_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= ID_MAX
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn random_hex() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_nanos())
        .unwrap_or(0);
    let mut hasher = RandomState::new().build_hasher();
    hasher.write_u64(COUNTER.fetch_add(1, Ordering::Relaxed));
    hasher.write_u128(nanos);
    format!("{:016x}", hasher.finish())
}

impl Meta {
    pub fn new(request_id: &str, trace_id: &str) -> Result<Meta, MetaError> {
        for (name, value) in [("request_id", request_id), ("trace_id", trace_id)] {
            if !good_id(value) {
                return Err(MetaError(format!(
                    "{name} {value:?} не годится: нужны 1..={ID_MAX} символов из A-Za-z0-9_-"
                )));
            }
        }
        Ok(Meta {
            request_id: request_id.to_string(),
            trace_id: trace_id.to_string(),
        })
    }

    pub fn fresh() -> Meta {
        Meta {
            request_id: format!("req-{}", random_hex()),
            trace_id: format!("trace-{}", random_hex()),
        }
    }

    pub fn follow(request_id: &str, trace_id: Option<&str>) -> Meta {
        let fresh = Meta::fresh();
        let request_id = match good_id(request_id) {
            true => request_id.to_string(),
            false => fresh.request_id,
        };
        let trace_id = match trace_id.filter(|trace| good_id(trace)) {
            Some(trace) => trace.to_string(),
            None => request_id.clone(),
        };
        Meta {
            request_id,
            trace_id,
        }
    }

    pub(crate) fn to_headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(REQUEST_ID, self.request_id.as_str());
        headers.insert(TRACE_ID, self.trace_id.as_str());
        headers
    }

    pub(crate) fn from_headers(headers: Option<&HeaderMap>) -> Option<Meta> {
        let headers = headers?;
        let request_id = headers.get(REQUEST_ID)?.as_str();
        let trace_id = headers.get(TRACE_ID)?.as_str();
        Meta::new(request_id, trace_id).ok()
    }
}
