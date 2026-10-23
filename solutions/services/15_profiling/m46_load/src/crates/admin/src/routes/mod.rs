use std::sync::Arc;

use contracts::{CheckView, LiveView, ReadyView};
use http_engine::{Request, Response, Router, handler, with_state};
use service_runtime::{Health, Report, metrics, record_runtime};

pub fn ready_view(report: &Report) -> ReadyView {
    ReadyView {
        ready: report.ready,
        stopping: report.stopping,
        checks: report
            .probes
            .iter()
            .map(|probe| CheckView {
                name: probe.name.clone(),
                ok: probe.outcome.is_ok(),
                reason: probe.outcome.clone().err(),
                millis: u64::try_from(probe.took.as_millis()).unwrap_or(u64::MAX),
            })
            .collect(),
    }
}

async fn live(_: Request) -> Response {
    Response::json(
        200,
        &LiveView {
            status: "live".to_string(),
        },
    )
}

async fn ready(health: Arc<Health>, _: Request) -> Response {
    let view = ready_view(&health.report().await);
    let status = if view.ready { 200 } else { 503 };
    Response::json(status, &view)
}

pub const METRICS_TYPE: &str = "text/plain; version=0.0.4; charset=utf-8";

async fn exposition(_: Request) -> Response {
    let mut response = Response::new(200);
    response.headers.set("Content-Type", METRICS_TYPE);
    record_runtime(metrics());
    response.body = metrics().render().into_bytes();
    response
}

pub fn router(health: Health) -> Router {
    Router::new()
        .get("/health", handler(live))
        .get("/metrics", handler(exposition))
        .get("/ready", with_state(Arc::new(health), ready))
}
