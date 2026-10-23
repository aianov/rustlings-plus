use std::sync::Arc;

use contracts::{ErrorBody, Outcome};
use grpc_kit::Stamp;
use http_engine::{
    BoxFuture, CorsPolicy, Handler, Request, Response, Router, access_log, apply,
    bearer_auth_async, cors, token_from_cookie, with_state,
};

use super::{avatars, posts, users};
use crate::bootstrap::Shared;

pub(crate) const PROTECTED: [&str; 2] = ["/users/me", "/users/me/avatar"];

pub(crate) const SESSION_COOKIE: &str = "session";

pub(crate) fn policy(origins: &[String]) -> CorsPolicy {
    let words = |list: &[&str]| list.iter().map(|word| word.to_string()).collect();
    CorsPolicy {
        origins: origins.to_vec(),
        methods: words(&["GET", "HEAD", "POST", "PUT", "DELETE"]),
        headers: words(&["Content-Type", "Authorization", "If-None-Match", "Range"]),
        expose_headers: words(&["ETag", "Content-Range", "Accept-Ranges"]),
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
                Ok(user) => Ok(user.map(|user| user.id.to_string())),
                Err(body) => Err(failure(body)),
            }
        })
    };
    let guard = |request: &Request| PROTECTED.contains(&request.raw_path());
    let mut layers = vec![cors(policy(&shared.config.cors_origins))];
    if shared.config.access_log {
        layers.push(access_log(Arc::new(|line| {
            eprintln!("api-gateway: {line}")
        })));
    }
    layers.push(token_from_cookie(SESSION_COOKIE));
    layers.push(bearer_auth_async(verify, guard));
    apply(router(shared).into_handler(), &layers)
}

pub(crate) fn router(shared: Arc<Shared>) -> Router {
    let state = |shared: &Arc<Shared>| Arc::clone(shared);
    Router::new()
        .get("/users/me", with_state(state(&shared), users::me))
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
        .get(
            "/avatars/:id",
            with_state(state(&shared), avatars::download),
        )
        .get("/ws", with_state(state(&shared), super::ws::open))
        .get("/posts/:id", with_state(state(&shared), posts::post))
        .get("/health", with_state(shared, health))
}

pub(crate) fn status_of(code: &str) -> u16 {
    match code {
        "bad_data" | "syntax" | "bad_envelope" | "truncated" => 400,
        "unauthorized" => 401,
        "forbidden" => 403,
        "not_found" => 404,
        "conflict" => 409,
        "too_large" => 413,
        "unsupported" => 415,
        "rejected" => 422,
        "unavailable" => 503,
        _ => 500,
    }
}

pub(crate) fn failure(body: ErrorBody) -> Response {
    let status = status_of(&body.code);
    Response::json(status, &Outcome::<()>::Error(body))
}

pub(crate) fn fail(code: &str, message: &str) -> Response {
    failure(ErrorBody {
        code: code.to_string(),
        message: message.to_string(),
    })
}

pub(crate) fn actor(request: &Request) -> Option<contracts::UserId> {
    request.headers.get("x-user")?.parse().ok()
}

pub(crate) fn stamp(shared: &Shared, request: &Request) -> Stamp {
    let stamp = Stamp::fresh().within(shared.config.budget);
    match actor(request) {
        Some(user) => stamp.user(user.0),
        None => stamp,
    }
}

async fn health(shared: Arc<Shared>, _: Request) -> Response {
    let missing = shared.readiness.missing();
    if missing.is_empty() {
        return Response::text(200, "ok");
    }
    fail(
        "unavailable",
        &format!("жду сервисы: {}", missing.join(", ")),
    )
}
