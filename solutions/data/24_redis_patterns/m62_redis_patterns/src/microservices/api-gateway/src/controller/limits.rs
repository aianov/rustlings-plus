use std::sync::Arc;
use std::time::Duration;

use contracts::ErrorCode;
use http_engine::{Middleware, Request, Response, handler, middleware};

use super::http::fail;
use crate::common::Guards;

fn retry_after(wait: Duration) -> String {
    wait.as_secs_f64().ceil().max(1.0).to_string()
}

pub(crate) fn shed() -> Response {
    let mut response = fail(
        ErrorCode::Unavailable,
        "шлюз перегружен: запрос сброшен сразу, повтори позже",
    );
    response.headers.set("Retry-After", "1");
    response
}

pub(crate) fn too_many(wait: Duration) -> Response {
    let mut response = fail(
        ErrorCode::Exhausted,
        "слишком много запросов с этого адреса, подожди",
    );
    response.headers.set("Retry-After", &retry_after(wait));
    response
}

pub(crate) fn guard_requests(guards: Arc<Guards>) -> Middleware {
    middleware(move |next| {
        let guards = Arc::clone(&guards);
        handler(move |request: Request| {
            let next = Arc::clone(&next);
            let guards = Arc::clone(&guards);
            async move {
                if request.raw_path() == "/health" {
                    return next(request).await;
                }
                let Some(_flight) = guards.admit() else {
                    return shed();
                };
                if let Some(peer) = request.peer {
                    if let Err(wait) = guards.take_ip(peer.ip()).await {
                        return too_many(wait);
                    }
                }
                next(request).await
            }
        })
    })
}
