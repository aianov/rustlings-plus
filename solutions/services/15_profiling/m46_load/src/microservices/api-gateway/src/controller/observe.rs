use std::sync::Arc;
use std::time::Duration;

use http_engine::{Middleware, Request, Served, handler, middleware};
use proto::REQUEST_ID;
use service_runtime::{TRACEPARENT, TraceContext, metrics, watch_polls, with_trace};
use tracing::Instrument;

pub(crate) const HTTP_REQUESTS: &str = "http_requests_total";

pub(crate) const HTTP_DURATION: &str = "http_request_duration_seconds";

pub(crate) const WS_REQUESTS: &str = "ws_requests_total";

pub(crate) const WS_DURATION: &str = "ws_request_duration_seconds";

pub(crate) const AUTH_ROUTE: &str = "/auth/*";

pub(crate) fn trace_requests() -> Middleware {
    middleware(|next| {
        handler(move |mut request: Request| {
            let next = Arc::clone(&next);
            let trace = TraceContext::continue_from(request.headers.get(TRACEPARENT));
            request.headers.set(TRACEPARENT, &trace.header());
            let span = tracing::info_span!(
                "http",
                method = %request.method,
                path = %request.raw_path(),
                request_id = request.headers.get(REQUEST_ID).unwrap_or("-"),
                trace_id = %trace.trace_id,
            );
            let header = trace.header();
            async move {
                let mut response =
                    watch_polls("http-request", with_trace(trace, next(request))).await;
                response.headers.set(TRACEPARENT, &header);
                if response.status.code() >= 500 {
                    tracing::warn!(status = response.status.code(), "ответ с ошибкой сервера");
                }
                response
            }
            .instrument(span)
        })
    })
}

pub(crate) fn observe_http(served: &Served) {
    let status = served.status.to_string();
    let method = served.method.as_str();
    metrics().inc(
        HTTP_REQUESTS,
        "HTTP-запросы шлюза по шаблону маршрута и статусу",
        &[
            ("method", method),
            ("route", &served.template),
            ("status", &status),
        ],
        1,
    );
    metrics().observe(
        HTTP_DURATION,
        "время HTTP-ответа шлюза в секундах",
        &[("method", method), ("route", &served.template)],
        served.elapsed.as_secs_f64(),
    );
}

pub(crate) fn observe_ws(method: &str, outcome: &str, elapsed: Duration) {
    metrics().inc(
        WS_REQUESTS,
        "запросы по WebSocket по методу и исходу",
        &[("method", method), ("outcome", outcome)],
        1,
    );
    metrics().observe(
        WS_DURATION,
        "время ответа на запрос по WebSocket в секундах",
        &[("method", method)],
        elapsed.as_secs_f64(),
    );
}
