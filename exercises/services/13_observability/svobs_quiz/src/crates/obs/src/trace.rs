#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceContext {
    pub trace_id: u128,
    pub span_id: u64,
    pub sampled: bool,
}

pub struct Entered {
    pub ctx: TraceContext,
    pub parent: Option<u64>,
}

pub fn parse(header: &str) -> Option<TraceContext> {
    let _ = header;
    todo!()
}

impl TraceContext {
    pub fn header(&self) -> String {
        todo!()
    }

    pub fn root(ids: &mut dyn FnMut() -> u64) -> TraceContext {
        let _ = ids;
        todo!()
    }

    pub fn child(&self, ids: &mut dyn FnMut() -> u64) -> TraceContext {
        let _ = ids;
        todo!()
    }
}

pub fn incoming(header: Option<&str>, ids: &mut dyn FnMut() -> u64) -> TraceContext {
    let _ = (header, ids);
    todo!()
}

pub fn enter(header: Option<&str>, ids: &mut dyn FnMut() -> u64) -> Entered {
    let _ = (header, ids);
    todo!()
}
