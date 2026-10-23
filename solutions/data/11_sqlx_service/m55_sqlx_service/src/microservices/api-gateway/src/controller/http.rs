use std::sync::Arc;

use contracts::{ErrorBody, ErrorCode, Outcome, parse_body};
use grpc_kit::Stamp;
use http_engine::{
    BoxFuture, CorsPolicy, Handler, Request, Response, Router, access_log, apply,
    bearer_auth_async, cors, handler, token_from_cookie, with_state,
};
use serde::de::DeserializeOwned;

use super::errors::{request_id, request_ids};
use super::limits::guard_requests;
use super::observe::{AUTH_ROUTE, observe_http, trace_requests};
use super::{avatars, drafts, export, posts, proxy, users};
use crate::bootstrap::Shared;

pub(crate) const PROTECTED: [&str; 3] = ["/users/me", "/users/me/avatar", export::EXPORT_PATH];

pub(crate) const OWNED: [&str; 2] = ["/drafts", "/comments/"];

pub(crate) const SESSION_COOKIE: &str = "session";

pub(crate) fn policy(origins: &[String]) -> CorsPolicy {
    let words = |list: &[&str]| list.iter().map(|word| word.to_string()).collect();
    CorsPolicy {
        origins: origins.to_vec(),
        methods: words(&["GET", "HEAD", "POST", "PUT", "PATCH", "DELETE"]),
        headers: words(&[
            "Content-Type",
            "Authorization",
            "If-None-Match",
            "Range",
            "X-Social-Client",
        ]),
        expose_headers: words(&["ETag", "Content-Range", "Accept-Ranges", "X-Request-Id"]),
        credentials: true,
        max_age: 600,
    }
}

pub(crate) fn app(shared: Arc<Shared>) -> Handler {
    let auth = shared.auth.clone();
    let budget = shared.config.budget;
    let verify = move |token: String| -> BoxFuture<Result<Option<String>, Response>> {
        let auth = auth.clone();
        Box::pin(async move {
            match auth.check(&token, &Stamp::fresh().within(budget)).await {
                Ok(user) => Ok(user.map(|principal| principal.id.to_string())),
                Err(body) => Err(failure(body)),
            }
        })
    };
    let guard = |request: &Request| {
        let path = request.raw_path();
        PROTECTED.contains(&path) || OWNED.iter().any(|prefix| path.starts_with(prefix))
    };
    let mut layers = vec![cors(policy(&shared.config.cors_origins)), request_ids()];
    layers.push(trace_requests());
    layers.push(guard_requests(Arc::clone(&shared.guards)));
    if shared.config.access_log {
        layers.push(access_log(Arc::new(
            |line| tracing::info!(target: "access", "{line}"),
        )));
    }
    layers.push(token_from_cookie(SESSION_COOKIE));
    layers.push(bearer_auth_async(verify, guard));
    let routes = router(Arc::clone(&shared)).into_measured(Arc::new(observe_http));
    let entry = handler(move |request: Request| {
        let shared = Arc::clone(&shared);
        let routes = Arc::clone(&routes);
        async move {
            if proxy::is_auth(&request) {
                let method = request.method;
                let started = tokio::time::Instant::now();
                let response = proxy::auth(shared, request).await;
                observe_http(&http_engine::Served {
                    method,
                    template: AUTH_ROUTE.to_string(),
                    status: response.status.code(),
                    elapsed: started.elapsed(),
                });
                response
            } else {
                routes(request).await
            }
        }
    });
    apply(entry, &layers)
}

pub(crate) fn router(shared: Arc<Shared>) -> Router {
    let state = |shared: &Arc<Shared>| Arc::clone(shared);
    Router::new()
        .get("/users/me", with_state(state(&shared), users::me))
        .route(
            http_engine::Method::Patch,
            "/users/me",
            with_state(state(&shared), users::update),
        )
        .get("/users/:username", with_state(state(&shared), users::user))
        .route(
            http_engine::Method::Put,
            "/users/me/avatar",
            with_state(state(&shared), avatars::upload),
        )
        .route(
            http_engine::Method::Delete,
            "/users/me/avatar",
            with_state(state(&shared), avatars::remove),
        )
        .get("/files/:key", with_state(state(&shared), avatars::download))
        .get(
            export::EXPORT_PATH,
            with_state(state(&shared), export::export),
        )
        .post("/drafts", with_state(state(&shared), drafts::save))
        .get("/drafts/:id", with_state(state(&shared), drafts::read))
        .route(
            http_engine::Method::Delete,
            "/drafts/:id",
            with_state(state(&shared), drafts::drop),
        )
        .route(
            http_engine::Method::Delete,
            "/comments/:id",
            with_state(state(&shared), drafts::uncomment),
        )
        .get("/ws", with_state(state(&shared), super::ws::open))
        .get("/posts/:id", with_state(state(&shared), posts::post))
        .get("/health", with_state(shared, health))
}

pub(crate) fn failure(body: ErrorBody) -> Response {
    Response::json(body.http_status(), &Outcome::<()>::Error(body))
}

pub(crate) fn fail(code: ErrorCode, message: &str) -> Response {
    failure(ErrorBody::new(code, message))
}

pub(crate) fn json<T: DeserializeOwned>(request: &Request) -> Result<T, Response> {
    let content_type = request.headers.get("content-type").unwrap_or_default();
    let media = content_type.split(';').next().unwrap_or_default().trim();
    if !media.eq_ignore_ascii_case("application/json") {
        return Err(fail(
            ErrorCode::Unsupported,
            "тело должно быть application/json",
        ));
    }
    parse_body(&request.body).map_err(failure)
}

pub(crate) fn actor(request: &Request) -> Option<contracts::UserId> {
    request.headers.get("x-user")?.parse().ok()
}

pub(crate) fn stamp(shared: &Shared, request: &Request) -> Stamp {
    let trace = service_runtime::TraceContext::continue_from(
        request.headers.get(service_runtime::TRACEPARENT),
    );
    let stamp = Stamp::new(&request_id(request))
        .within(shared.config.budget)
        .in_trace(trace);
    match actor(request) {
        Some(user) => stamp.user(user.0),
        None => stamp,
    }
}

async fn health(shared: Arc<Shared>, _: Request) -> Response {
    let mut missing = shared.readiness.missing();
    if !shared.upstreams.post_replicas().healthy().is_empty() {
        missing.retain(|label| *label != crate::client::REPLICA);
    }
    if missing.is_empty() {
        return Response::text(200, "ok");
    }
    fail(
        ErrorCode::Unavailable,
        &format!("жду сервисы: {}", missing.join(", ")),
    )
}
