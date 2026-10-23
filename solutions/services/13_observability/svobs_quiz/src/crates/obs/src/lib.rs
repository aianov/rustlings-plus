pub mod logs;
pub mod metrics;
pub mod red;
pub mod redact;
pub mod trace;

pub const TRACEPARENT: &str = "traceparent";

pub fn hop_span(service: &'static str, ctx: &trace::TraceContext, parent: Option<u64>) -> tracing::Span {
    tracing::info_span!(
        "hop",
        service,
        trace_id = %format!("{:032x}", ctx.trace_id),
        span_id = %format!("{:016x}", ctx.span_id),
        parent_id = %parent.map(|p| format!("{p:016x}")).unwrap_or_default()
    )
}
