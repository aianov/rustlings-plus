use std::collections::hash_map::RandomState;
use std::future::Future;
use std::hash::{BuildHasher, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub const TRACEPARENT: &str = "traceparent";

tokio::task_local! {
    static CURRENT: TraceContext;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceContext {
    pub trace_id: String,
    pub span_id: String,
    pub sampled: bool,
}

fn random_u64() -> u64 {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    let mut hasher = RandomState::new().build_hasher();
    hasher.write_u64(COUNTER.fetch_add(1, Ordering::Relaxed));
    hasher.write_u128(nanos);
    hasher.finish()
}

fn nonzero_hex(len: usize) -> String {
    loop {
        let mut out = String::with_capacity(len);
        while out.len() < len {
            out.push_str(&format!("{:016x}", random_u64()));
        }
        out.truncate(len);
        if out.bytes().any(|byte| byte != b'0') {
            return out;
        }
    }
}

fn lower_hex(value: &str, len: usize) -> bool {
    value.len() == len
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        && value.bytes().any(|byte| byte != b'0')
}

impl TraceContext {
    pub fn fresh() -> TraceContext {
        TraceContext {
            trace_id: nonzero_hex(32),
            span_id: nonzero_hex(16),
            sampled: true,
        }
    }

    pub fn parse(header: &str) -> Option<TraceContext> {
        let mut parts = header.trim().split('-');
        let version = parts.next()?;
        let trace_id = parts.next()?;
        let span_id = parts.next()?;
        let flags = parts.next()?;
        if parts.next().is_some() && version == "00" {
            return None;
        }
        if version.len() != 2 || version == "ff" || !version.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return None;
        }
        if !lower_hex(trace_id, 32) || !lower_hex(span_id, 16) || flags.len() != 2 {
            return None;
        }
        let flags = u8::from_str_radix(flags, 16).ok()?;
        Some(TraceContext {
            trace_id: trace_id.to_string(),
            span_id: span_id.to_string(),
            sampled: flags & 1 == 1,
        })
    }

    pub fn child(&self) -> TraceContext {
        TraceContext {
            trace_id: self.trace_id.clone(),
            span_id: nonzero_hex(16),
            sampled: self.sampled,
        }
    }

    pub fn header(&self) -> String {
        let flags = if self.sampled { "01" } else { "00" };
        format!("00-{}-{}-{flags}", self.trace_id, self.span_id)
    }

    pub fn continue_from(header: Option<&str>) -> TraceContext {
        header
            .and_then(TraceContext::parse)
            .map_or_else(TraceContext::fresh, |parent| parent.child())
    }
}

pub fn current_trace() -> Option<TraceContext> {
    CURRENT.try_with(Clone::clone).ok()
}

pub fn with_trace<F: Future>(trace: TraceContext, work: F) -> impl Future<Output = F::Output> {
    CURRENT.scope(trace, work)
}
