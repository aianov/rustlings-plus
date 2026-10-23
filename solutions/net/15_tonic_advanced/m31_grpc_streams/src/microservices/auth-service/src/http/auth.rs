use std::sync::Arc;

use contracts::{Credentials, Login, UserId, UserView};
use grpc_kit::Stamp;
use http_engine::{Request, Response, SameSite, SetCookie};
use proto::explain;
use proto::social::v1::CreateProfileRequest;
use proto::social::v1::users_client::UsersClient;

use super::fail;
use crate::accounts::Account;
use crate::state::Shared;

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
        Err(error) => fail("internal", &error.to_string()),
    }
}

fn view(account: &Account) -> UserView {
    UserView {
        id: UserId(account.user_id),
        username: account.username.clone(),
    }
}

pub(crate) async fn register(shared: Arc<Shared>, request: Request) -> Response {
    let credentials: Credentials = match request.json() {
        Ok(credentials) => credentials,
        Err(response) => return response,
    };
    let stamp = Stamp::fresh();
    let rid = stamp.request_id().to_string();
    let ask = CreateProfileRequest {
        username: credentials.username.as_ref().to_string(),
    };
    let mut users = UsersClient::with_interceptor(shared.users.clone(), stamp);
    let created = users.create_profile(ask).await;
    let profile = match created {
        Ok(profile) => profile.into_inner(),
        Err(status) => {
            let (code, message) = explain(&status, "сервис профилей");
            if code == "internal" {
                eprintln!("auth-service [{rid}]: user-service ответил {status}");
            }
            return fail(&code, &message);
        }
    };
    let account = Account {
        user_id: profile.id,
        username: profile.username,
    };
    if let Err(error) = shared.accounts.add(account.clone()).await {
        eprintln!("auth-service [{rid}]: журнал учётных записей: {error}");
        return fail(
            "internal",
            "учётная запись не сохранилась, попробуй ещё раз",
        );
    }
    Response::json(201, &view(&account))
}

pub(crate) async fn login(shared: Arc<Shared>, request: Request) -> Response {
    let credentials: Credentials = match request.json() {
        Ok(credentials) => credentials,
        Err(response) => return response,
    };
    let name = credentials.username.as_ref();
    let Some(account) = shared.accounts.find(name) else {
        return fail(
            "unauthorized",
            &format!("пользователя @{name} нет, сначала /auth/register"),
        );
    };
    match shared.tokens.issue(account.clone()) {
        Ok(token) => {
            let cookie = session_cookie(&token);
            let login = Login {
                token,
                user: view(&account),
            };
            with_cookie(Response::json(200, &login), &cookie)
        }
        Err(..) => fail("internal", "генератор случайных чисел недоступен"),
    }
}

pub(crate) async fn logout(shared: Arc<Shared>, request: Request) -> Response {
    let token = request
        .headers
        .get("authorization")
        .and_then(|value| value.split_once(' '))
        .map(|(_, token)| token.trim().to_string())
        .unwrap_or_default();
    shared.tokens.revoke(&token);
    with_cookie(Response::new(204), &SetCookie::delete(SESSION_COOKIE, "/"))
}
