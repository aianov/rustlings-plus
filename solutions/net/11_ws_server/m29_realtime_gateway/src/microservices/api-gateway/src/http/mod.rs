mod auth;
mod avatars;
mod posts;
mod users;

use std::sync::Arc;

use contracts::{ErrorBody, Outcome};
use http_engine::{
    CorsPolicy, Handler, Request, Response, Router, access_log, apply, bearer_auth, cors,
    token_from_cookie, with_state,
};

use crate::state::Shared;

pub(crate) const PROTECTED: [&str; 3] = ["/users/me", "/users/me/avatar", "/auth/logout"];

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
    let auth = Arc::clone(&shared.auth);
    let verify = move |token: &str| auth.check(token).map(|user| user.id.to_string());
    let guard = |request: &Request| PROTECTED.contains(&request.raw_path());
    let mut layers = vec![cors(policy(&shared.config.cors_origins))];
    if shared.config.access_log {
        layers.push(access_log(Arc::new(|line| {
            eprintln!("api-gateway: {line}")
        })));
    }
    layers.push(token_from_cookie(SESSION_COOKIE));
    layers.push(bearer_auth(verify, guard));
    apply(router(shared).into_handler(), &layers)
}

pub(crate) fn router(shared: Arc<Shared>) -> Router {
    let state = |shared: &Arc<Shared>| Arc::clone(shared);
    Router::new()
        .post("/auth/register", with_state(state(&shared), auth::register))
        .post("/auth/login", with_state(state(&shared), auth::login))
        .post("/auth/logout", with_state(state(&shared), auth::logout))
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
        .get("/ws", with_state(state(&shared), crate::ws::open))
        .get("/posts/:id", with_state(shared, posts::post))
        .get("/health", with_state(Arc::new(()), health))
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

async fn health(_: Arc<()>, _: Request) -> Response {
    Response::text(200, "ok")
}
