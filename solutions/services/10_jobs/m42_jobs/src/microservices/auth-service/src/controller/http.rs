use std::sync::Arc;

use contracts::{ErrorBody, ErrorCode, Outcome, parse_body};
use http_engine::{
    BoxFuture, CorsPolicy, Handler, Request, Response, Router, access_log, apply,
    bearer_auth_async, cors, token_from_cookie, with_state,
};
use serde::de::DeserializeOwned;

use super::handlers::{self, SESSION_COOKIE};
use crate::bootstrap::Shared;

const PROTECTED: [&str; 5] = [
    "/auth/logout",
    "/auth/logout-all",
    "/auth/password",
    "/auth/email/request",
    "/auth/delete-account",
];

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
            match service.validate(&token).await {
                Ok(principal) => Ok(Some(principal.user_id.to_string())),
                Err(error) => Err(failure(error.body())),
            }
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
        .post(
            "/auth/refresh",
            with_state(state(&shared), handlers::refresh),
        )
        .post("/auth/logout", with_state(state(&shared), handlers::logout))
        .post(
            "/auth/logout-all",
            with_state(state(&shared), handlers::logout_all),
        )
        .post(
            "/auth/password",
            with_state(state(&shared), handlers::change_password),
        )
        .post(
            "/auth/delete-account",
            with_state(state(&shared), handlers::delete_account),
        )
        .post(
            "/auth/password/forgot",
            with_state(state(&shared), handlers::forgot),
        )
        .post(
            "/auth/password/reset",
            with_state(state(&shared), handlers::reset),
        )
        .post(
            "/auth/email/request",
            with_state(state(&shared), handlers::request_confirmation),
        )
        .get(
            "/auth/email/confirm",
            with_state(state(&shared), handlers::confirm_link),
        )
        .post(
            "/auth/email/confirm",
            with_state(shared, handlers::confirm_body),
        )
        .get("/health", with_state(Arc::new(()), health))
}

pub fn failure(body: ErrorBody) -> Response {
    let mut response = Response::json(body.http_status(), &Outcome::<()>::Error(body.clone()));
    if let Some(rid) = &body.request_id {
        response.headers.append("x-request-id", rid);
    }
    response
}

pub fn fail(code: ErrorCode, message: &str) -> Response {
    failure(ErrorBody::new(code, message))
}

pub fn json<T: DeserializeOwned>(request: &Request, rid: &str) -> Result<T, Response> {
    let content_type = request.headers.get("content-type").unwrap_or_default();
    let media = content_type.split(';').next().unwrap_or_default().trim();
    if !media.eq_ignore_ascii_case("application/json") {
        let body = ErrorBody::new(ErrorCode::Unsupported, "тело должно быть application/json");
        return Err(failure(body.with_request_id(rid)));
    }
    parse_body(&request.body).map_err(|body| failure(body.with_request_id(rid)))
}

async fn health(_: Arc<()>, _: Request) -> Response {
    Response::text(200, "ok")
}
