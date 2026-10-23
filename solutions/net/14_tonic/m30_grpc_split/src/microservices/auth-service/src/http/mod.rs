mod auth;

use std::sync::Arc;

use contracts::{ErrorBody, Outcome};
use http_engine::{
    CorsPolicy, Handler, Request, Response, Router, access_log, apply, bearer_auth, cors,
    token_from_cookie, with_state,
};

use crate::state::Shared;

pub use auth::{SESSION_COOKIE, SESSION_TTL};

pub(crate) const PROTECTED: [&str; 1] = ["/auth/logout"];

pub(crate) fn policy(origins: &[String]) -> CorsPolicy {
    let words = |list: &[&str]| list.iter().map(|word| word.to_string()).collect();
    CorsPolicy {
        origins: origins.to_vec(),
        methods: words(&["GET", "POST"]),
        headers: words(&["Content-Type", "Authorization"]),
        expose_headers: Vec::new(),
        credentials: true,
        max_age: 600,
    }
}

pub(crate) fn app(shared: Arc<Shared>) -> Handler {
    let tokens = Arc::clone(&shared.tokens);
    let verify = move |token: &str| {
        tokens
            .account(token)
            .map(|account| account.user_id.to_string())
    };
    let guard = |request: &Request| PROTECTED.contains(&request.raw_path());
    let mut layers = vec![cors(policy(&shared.config.cors_origins))];
    if shared.config.access_log {
        layers.push(access_log(Arc::new(|line| {
            eprintln!("auth-service: {line}")
        })));
    }
    layers.push(token_from_cookie(SESSION_COOKIE));
    layers.push(bearer_auth(verify, guard));
    apply(router(shared).into_handler(), &layers)
}

fn router(shared: Arc<Shared>) -> Router {
    let state = |shared: &Arc<Shared>| Arc::clone(shared);
    Router::new()
        .post("/auth/register", with_state(state(&shared), auth::register))
        .post("/auth/login", with_state(state(&shared), auth::login))
        .post("/auth/logout", with_state(shared, auth::logout))
        .get("/health", with_state(Arc::new(()), health))
}

pub(crate) fn status_of(code: &str) -> u16 {
    match code {
        "bad_data" | "syntax" | "bad_envelope" | "truncated" => 400,
        "unauthorized" => 401,
        "forbidden" => 403,
        "not_found" => 404,
        "conflict" => 409,
        "rejected" => 422,
        "unavailable" => 503,
        _ => 500,
    }
}

pub(crate) fn fail(code: &str, message: &str) -> Response {
    let body = ErrorBody {
        code: code.to_string(),
        message: message.to_string(),
    };
    Response::json(status_of(code), &Outcome::<()>::Error(body))
}

async fn health(_: Arc<()>, _: Request) -> Response {
    Response::text(200, "ok")
}
