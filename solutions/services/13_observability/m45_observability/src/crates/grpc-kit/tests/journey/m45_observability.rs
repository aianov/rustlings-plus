use grpc_kit::Stamp;
use service_runtime::{TRACEPARENT, TraceContext, with_trace};
use tonic::Request;
use tonic::service::Interceptor;

fn sent(mut stamp: Stamp) -> Option<TraceContext> {
    let request = stamp.call(Request::new(())).expect("штамп проходит");
    request
        .metadata()
        .get(TRACEPARENT)
        .and_then(|value| value.to_str().ok())
        .and_then(TraceContext::parse)
}

#[test]
fn stamp_carries_traceparent() {
    let trace = TraceContext::fresh();
    assert_eq!(
        sent(Stamp::fresh().in_trace(trace.clone())),
        Some(trace.clone())
    );
    assert_eq!(sent(Stamp::fresh()), None, "без трейса заголовка нет");
    let incoming = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";
    let passed = sent(Stamp::fresh().pass("traceparent", incoming)).expect("трейс из конверта");
    assert_eq!(passed.trace_id, "4bf92f3577b34da6a3ce929d0e0e4736");
    assert_ne!(
        passed.span_id, "00f067aa0ba902b7",
        "свой span: ребёнок пришедшего"
    );
    assert_eq!(sent(Stamp::fresh().pass("traceparent", "кривой")), None);
}

#[test]
fn follow_continues_the_incoming_trace() {
    let trace = TraceContext::fresh();
    let mut request = Request::new(());
    request.metadata_mut().insert(
        TRACEPARENT,
        trace.header().parse().expect("заголовок ASCII"),
    );
    let followed = Stamp::follow(&request);
    assert_eq!(
        followed.trace(),
        Some(&trace),
        "следующий сервис продолжает тот же трейс"
    );
}

#[tokio::test]
async fn ambient_trace_reaches_the_next_call() {
    let trace = TraceContext::fresh();
    let seen = with_trace(trace.clone(), async { sent(Stamp::fresh()) }).await;
    assert_eq!(seen.map(|t| t.trace_id), Some(trace.trace_id));
}
