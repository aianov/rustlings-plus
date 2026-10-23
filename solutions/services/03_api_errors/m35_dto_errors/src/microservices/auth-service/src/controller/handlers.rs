use std::sync::Arc;

use contracts::{ErrorBody, ErrorCode, Login};
use grpc_kit::Stamp;
use http_engine::{Request, Response, SameSite, SetCookie};

use super::http::{fail, failure, json};
use crate::bootstrap::Shared;
use crate::client::GrpcProfiles;
use crate::dto::{self, LoginBody, RegistrationBody};
use crate::service::AuthError;

pub const SESSION_COOKIE: &str = "session";

pub const SESSION_TTL: i64 = 86_400;

fn session_cookie(token: &str) -> SetCookie {
    SetCookie {
        max_age: Some(SESSION_TTL),
        path: Some(String::from("/")),
        http_only: true,
        same_site: Some(SameSite::Lax),
        ..SetCookie::new(SESSION_COOKIE, token)
    }
}

fn with_cookie(mut response: Response, cookie: &SetCookie) -> Response {
    match cookie.to_header() {
        Ok(header) => {
            response.headers.append("Set-Cookie", &header);
            response
        }
        Err(error) => fail(ErrorCode::Internal, &error.to_string()),
    }
}

fn refused(error: &AuthError, rid: &str) -> Response {
    if error.code() == ErrorCode::Internal {
        eprintln!("auth-service [{rid}]: {}", service_runtime::report(error));
    }
    failure(error.body().with_request_id(rid))
}

fn invalid(errors: &validation::Invalid, rid: &str) -> Response {
    failure(ErrorBody::invalid(errors).with_request_id(rid))
}

pub(crate) async fn register(shared: Arc<Shared>, request: Request) -> Response {
    let stamp = Stamp::fresh();
    let rid = stamp.request_id().to_string();
    let body: RegistrationBody = match json(&request, &rid) {
        Ok(body) => body,
        Err(response) => return response,
    };
    let registration = match dto::registration(&body) {
        Ok(registration) => registration,
        Err(errors) => return invalid(&errors, &rid),
    };
    let profiles = GrpcProfiles::new(shared.users.clone(), stamp);
    let service = Arc::clone(&shared.service);
    let job = tokio::spawn(async move { service.register(&profiles, &registration).await });
    match job.await {
        Ok(Ok(account)) => Response::json(201, &dto::view(&account)),
        Ok(Err(error)) => refused(&error, &rid),
        Err(..) => failure(ErrorBody::public(ErrorCode::Internal).with_request_id(&rid)),
    }
}

pub(crate) async fn login(shared: Arc<Shared>, request: Request) -> Response {
    let rid = Stamp::fresh().request_id().to_string();
    let body: LoginBody = match json(&request, &rid) {
        Ok(body) => body,
        Err(response) => return response,
    };
    let name = match dto::login(&body) {
        Ok(name) => name,
        Err(errors) => return invalid(&errors, &rid),
    };
    match shared.service.login(&name).await {
        Ok((token, account)) => {
            let cookie = session_cookie(&token);
            let login = Login {
                token,
                user: dto::view(&account),
            };
            with_cookie(Response::json(200, &login), &cookie)
        }
        Err(error) => refused(&error, &rid),
    }
}

pub(crate) async fn logout(shared: Arc<Shared>, request: Request) -> Response {
    let token = request
        .headers
        .get("authorization")
        .and_then(|value| value.split_once(' '))
        .map(|(_, token)| token.trim().to_string())
        .unwrap_or_default();
    shared.service.logout(&token).await;
    with_cookie(Response::new(204), &SetCookie::delete(SESSION_COOKIE, "/"))
}
