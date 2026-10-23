use std::sync::Arc;

use contracts::{ErrorBody, ErrorCode, Login};
use grpc_kit::Stamp;
use http_engine::{Request, Response, SameSite, SetCookie, request_cookie};
use proto::{is_request_id, new_request_id};

use super::http::{fail, failure, json};
use crate::bootstrap::Shared;
use crate::client::GrpcProfiles;
use crate::dto::{
    self, ConfirmBody, ForgotBody, LoginBody, PasswordChangeBody, RegistrationBody, ResetBody,
};
use crate::service::{AuthError, Issued};

pub const SESSION_COOKIE: &str = "session";

pub const REFRESH_COOKIE: &str = "refresh";

pub const REFRESH_PATH: &str = "/auth";

pub const SESSION_TTL: i64 = 86_400;

fn seconds(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

fn cookie(name: &str, value: &str, path: &str, age: u64, same_site: SameSite) -> SetCookie {
    SetCookie {
        max_age: Some(seconds(age)),
        path: Some(path.to_string()),
        http_only: true,
        same_site: Some(same_site),
        ..SetCookie::new(name, value)
    }
}

fn with_cookies(mut response: Response, cookies: &[SetCookie]) -> Response {
    for cookie in cookies {
        match cookie.to_header() {
            Ok(header) => response.headers.append("Set-Cookie", &header),
            Err(error) => return fail(ErrorCode::Internal, &error.to_string()),
        }
    }
    response
}

fn gone() -> [SetCookie; 2] {
    [
        SetCookie::delete(SESSION_COOKIE, "/"),
        SetCookie::delete(REFRESH_COOKIE, REFRESH_PATH),
    ]
}

fn refused(error: &AuthError, rid: &str) -> Response {
    if error.code() == ErrorCode::Internal {
        tracing::warn!("auth-service [{rid}]: {}", service_runtime::report(error));
    }
    failure(error.body().with_request_id(rid))
}

fn invalid(errors: &validation::Invalid, rid: &str) -> Response {
    failure(ErrorBody::invalid(errors).with_request_id(rid))
}

fn signed_in(shared: &Shared, issued: Issued, status: u16) -> Response {
    let lifetimes = shared.service.lifetimes();
    let login = Login {
        token: issued.access.clone(),
        user: dto::view(&issued.account),
        expires_in: issued.expires_in,
    };
    let cookies = [
        cookie(
            SESSION_COOKIE,
            &issued.access,
            "/",
            lifetimes.access,
            SameSite::Lax,
        ),
        cookie(
            REFRESH_COOKIE,
            &issued.refresh,
            REFRESH_PATH,
            lifetimes.refresh,
            SameSite::Strict,
        ),
    ];
    with_cookies(Response::json(status, &login), &cookies)
}

fn bearer(request: &Request) -> String {
    request
        .headers
        .get("authorization")
        .and_then(|value| value.split_once(' '))
        .map(|(_, token)| token.trim().to_string())
        .unwrap_or_default()
}

fn rid(request: &Request) -> String {
    request
        .headers
        .get("x-request-id")
        .filter(|rid| is_request_id(rid))
        .map_or_else(new_request_id, str::to_string)
}

pub(crate) async fn register(shared: Arc<Shared>, request: Request) -> Response {
    let rid = rid(&request);
    let stamp = Stamp::new(&rid);
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
    let rid = rid(&request);
    let body: LoginBody = match json(&request, &rid) {
        Ok(body) => body,
        Err(response) => return response,
    };
    let (name, password) = match dto::login(&body) {
        Ok(parsed) => parsed,
        Err(errors) => return invalid(&errors, &rid),
    };
    match shared
        .service
        .login_from(&name, &password, client_address(&request))
        .await
    {
        Ok(issued) => signed_in(&shared, issued, 200),
        Err(error) => refused(&error, &rid),
    }
}

pub const CLIENT_HEADER: &str = "X-Social-Client";

fn client_address(request: &Request) -> Option<std::net::IpAddr> {
    request
        .headers
        .get("x-real-ip")
        .and_then(|raw| raw.trim().parse().ok())
        .or_else(|| request.peer.map(|peer| peer.ip()))
}

pub(crate) async fn refresh(shared: Arc<Shared>, request: Request) -> Response {
    let rid = rid(&request);
    if request.headers.get(CLIENT_HEADER).is_none_or(str::is_empty) {
        let body = ErrorBody::new(
            ErrorCode::Forbidden,
            "refresh по cookie только со своим заголовком X-Social-Client: чужая страница его не пришлёт",
        );
        return failure(body.with_request_id(&rid));
    }
    let Some(presented) = request_cookie(&request, REFRESH_COOKIE) else {
        let body = ErrorBody::new(ErrorCode::Unauthorized, "нет cookie refresh: войди заново");
        return failure(body.with_request_id(&rid));
    };
    match shared.service.refresh(&presented).await {
        Ok(issued) => signed_in(&shared, issued, 200),
        Err(error) => with_cookies(refused(&error, &rid), &gone()),
    }
}

pub(crate) async fn logout(shared: Arc<Shared>, request: Request) -> Response {
    let rid = rid(&request);
    match shared.service.logout(&bearer(&request)).await {
        Ok(()) => with_cookies(Response::new(204), &gone()),
        Err(error) => refused(&error, &rid),
    }
}

pub(crate) async fn logout_all(shared: Arc<Shared>, request: Request) -> Response {
    let rid = rid(&request);
    match shared.service.logout_all(&bearer(&request)).await {
        Ok(..) => with_cookies(Response::new(204), &gone()),
        Err(error) => refused(&error, &rid),
    }
}

pub(crate) async fn change_password(shared: Arc<Shared>, request: Request) -> Response {
    let rid = rid(&request);
    let body: PasswordChangeBody = match json(&request, &rid) {
        Ok(body) => body,
        Err(response) => return response,
    };
    let (old, new) = match dto::password_change(&body) {
        Ok(parsed) => parsed,
        Err(errors) => return invalid(&errors, &rid),
    };
    let access = bearer(&request);
    match shared.service.change_password(&access, &old, &new).await {
        Ok(..) => Response::new(204),
        Err(error) => refused(&error, &rid),
    }
}

pub(crate) async fn request_confirmation(shared: Arc<Shared>, request: Request) -> Response {
    let rid = rid(&request);
    match shared.service.request_confirmation(&bearer(&request)).await {
        Ok(()) => Response::new(202),
        Err(error) => refused(&error, &rid),
    }
}

async fn confirm(shared: &Shared, token: &str, rid: &str) -> Response {
    match shared.service.confirm_email(token).await {
        Ok(()) => Response::text(200, "почта подтверждена"),
        Err(error) => refused(&error, rid),
    }
}

pub(crate) async fn confirm_link(shared: Arc<Shared>, request: Request) -> Response {
    let rid = rid(&request);
    let token = request
        .query()
        .ok()
        .and_then(|pairs| pairs.into_iter().find(|(name, _)| name == "token"))
        .map(|(_, value)| value)
        .unwrap_or_default();
    confirm(&shared, &token, &rid).await
}

pub(crate) async fn confirm_body(shared: Arc<Shared>, request: Request) -> Response {
    let rid = rid(&request);
    match json::<ConfirmBody>(&request, &rid) {
        Ok(body) => confirm(&shared, &body.token, &rid).await,
        Err(response) => response,
    }
}

pub(crate) async fn forgot(shared: Arc<Shared>, request: Request) -> Response {
    let rid = rid(&request);
    let body: ForgotBody = match json(&request, &rid) {
        Ok(body) => body,
        Err(response) => return response,
    };
    let name = match dto::forgot(&body) {
        Ok(name) => name,
        Err(errors) => return invalid(&errors, &rid),
    };
    match shared.service.forgot_password(&name).await {
        Ok(()) => Response::new(202),
        Err(error) => refused(&error, &rid),
    }
}

pub(crate) async fn reset(shared: Arc<Shared>, request: Request) -> Response {
    let rid = rid(&request);
    let body: ResetBody = match json(&request, &rid) {
        Ok(body) => body,
        Err(response) => return response,
    };
    let (token, password) = match dto::reset(&body) {
        Ok(parsed) => parsed,
        Err(errors) => return invalid(&errors, &rid),
    };
    match shared.service.reset_password(&token, &password).await {
        Ok(..) => Response::new(204),
        Err(error) => refused(&error, &rid),
    }
}

pub(crate) async fn delete_account(shared: Arc<Shared>, request: Request) -> Response {
    let rid = rid(&request);
    let Some(queue) = shared.deletions.as_deref() else {
        let body = ErrorBody::new(
            ErrorCode::Unavailable,
            "удаление аккаунтов выключено: auth-service запущен без NATS_URL",
        );
        return failure(body.with_request_id(&rid));
    };
    match shared
        .service
        .request_deletion(&bearer(&request), queue)
        .await
    {
        Ok(saga) => Response::json(202, &serde_json::json!({ "saga": saga })),
        Err(error) => refused(&error, &rid),
    }
}
