#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceContext {
    pub trace_id: u128,
    pub span_id: u64,
    pub sampled: bool,
}

fn hex_lower(text: &str, len: usize) -> bool {
    text.len() == len && text.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub fn parse(header: &str) -> Option<TraceContext> {
    let mut parts = header.splitn(5, '-');
    let version = parts.next()?;
    let trace = parts.next()?;
    let span = parts.next()?;
    let flags = parts.next()?;
    let rest = parts.next();
    if !hex_lower(version, 2) || version == "ff" || !hex_lower(trace, 32) || !hex_lower(span, 16) || !hex_lower(flags, 2) {
        return None;
    }
    if version == "00" && rest.is_some() {
        return None;
    }
    let trace_id = u128::from_str_radix(trace, 16).ok()?;
    let span_id = u64::from_str_radix(span, 16).ok()?;
    if trace_id == 0 || span_id == 0 {
        return None;
    }
    let sampled = u8::from_str_radix(flags, 16).ok()? & 1 == 1;
    Some(TraceContext { trace_id, span_id, sampled })
}

impl TraceContext {
    pub fn header(&self) -> String {
        format!("00-{:032x}-{:016x}-{:02x}", self.trace_id, self.span_id, u8::from(self.sampled))
    }

    pub fn root(ids: &mut dyn FnMut() -> u64) -> TraceContext {
        let mut trace_id = 0u128;
        while trace_id == 0 {
            trace_id = (u128::from(ids()) << 64) | u128::from(ids());
        }
        TraceContext { trace_id, span_id: fresh_span_id(ids, 0), sampled: true }
    }

    pub fn child(&self, ids: &mut dyn FnMut() -> u64) -> TraceContext {
        TraceContext { trace_id: self.trace_id, span_id: fresh_span_id(ids, self.span_id), sampled: self.sampled }
    }
}

fn fresh_span_id(ids: &mut dyn FnMut() -> u64, avoid: u64) -> u64 {
    loop {
        let id = ids();
        if id != 0 && id != avoid {
            return id;
        }
    }
}

pub fn incoming(header: Option<&str>, ids: &mut dyn FnMut() -> u64) -> TraceContext {
    match header.and_then(parse) {
        Some(parent) => parent.child(ids),
        None => TraceContext::root(ids),
    }
}

pub struct Entered {
    pub ctx: TraceContext,
    pub parent: Option<u64>,
}

pub fn enter(header: Option<&str>, ids: &mut dyn FnMut() -> u64) -> Entered {
    let parent = header.and_then(parse).map(|p| p.span_id);
    Entered { ctx: incoming(header, ids), parent }
}
