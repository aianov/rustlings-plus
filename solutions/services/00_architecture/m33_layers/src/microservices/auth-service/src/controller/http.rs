use std::sync::Arc;

use contracts::{ErrorBody, Outcome};
use http_engine::{
    BoxFuture, CorsPolicy, Handler, Request, Response, Router, access_log, apply,
    bearer_auth_async, cors, token_from_cookie, with_state,
};

use super::handlers::{self, SESSION_COOKIE};
use crate::bootstrap::Shared;

const PROTECTED: [&str; 1] = ["/auth/logout"];

fn policy(origins: &[String]) -> CorsPolicy {
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

pub fn app(shared: Arc<Shared>) -> Handler {
    let service = Arc::clone(&shared.service);
    let verify = move |token: String| -> BoxFuture<Result<Option<String>, Response>> {
        let service = Arc::clone(&service);
        Box::pin(async move {
            let account = service.validate(&token).await;
            Ok(account.map(|account| account.user_id.to_string()))
        })
    };
    let guard = |request: &Request| PROTECTED.contains(&request.raw_path());
    let mut layers = vec![cors(policy(&shared.config.cors_origins))];
    if shared.config.access_log {
        layers.push(access_log(Arc::new(|line| {
            eprintln!("auth-service: {line}")
        })));
    }
    layers.push(token_from_cookie(SESSION_COOKIE));
    layers.push(bearer_auth_async(verify, guard));
    apply(router(shared).into_handler(), &layers)
}

fn router(shared: Arc<Shared>) -> Router {
    let state = |shared: &Arc<Shared>| Arc::clone(shared);
    Router::new()
        .post(
            "/auth/register",
            with_state(state(&shared), handlers::register),
        )
        .post("/auth/login", with_state(state(&shared), handlers::login))
        .post("/auth/logout", with_state(shared, handlers::logout))
        .get("/health", with_state(Arc::new(()), health))
}

pub fn status_of(code: &str) -> u16 {
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

pub fn fail(code: &str, message: &str) -> Response {
    let body = ErrorBody {
        code: code.to_string(),
        message: message.to_string(),
    };
    Response::json(status_of(code), &Outcome::<()>::Error(body))
}

async fn health(_: Arc<()>, _: Request) -> Response {
    Response::text(200, "ok")
}
