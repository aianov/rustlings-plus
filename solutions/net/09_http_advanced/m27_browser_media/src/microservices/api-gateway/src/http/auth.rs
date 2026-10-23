use std::sync::Arc;

use contracts::{Credentials, Login, UserView};
use http_engine::{Request, Response, SameSite, SetCookie};

use super::{SESSION_COOKIE, fail, failure};
use crate::server::Shared;

const SESSION_AGE: i64 = 86_400;

fn session_cookie(token: &str) -> SetCookie {
    SetCookie {
        max_age: Some(SESSION_AGE),
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

pub(crate) async fn register(shared: Arc<Shared>, request: Request) -> Response {
    let credentials: Credentials = match request.json() {
        Ok(credentials) => credentials,
        Err(response) => return response,
    };
    let name = credentials.username;
    match shared.app.register(name.as_ref()).await {
        Ok(id) => Response::json(
            201,
            &UserView {
                id,
                username: name.as_ref().to_string(),
            },
        ),
        Err(error) => failure(social_core::Error::from(error).to_body()),
    }
}

pub(crate) async fn login(shared: Arc<Shared>, request: Request) -> Response {
    let credentials: Credentials = match request.json() {
        Ok(credentials) => credentials,
        Err(response) => return response,
    };
    let name = credentials.username.as_ref();
    let Some(found) = shared.app.find_user(name).await else {
        return fail(
            "unauthorized",
            &format!("пользователя @{name} нет, сначала /auth/register"),
        );
    };
    let user = UserView {
        id: found.id(),
        username: found.name().to_string(),
    };
    match shared.tokens.issue(user.clone()) {
        Ok(token) => {
            let cookie = session_cookie(&token);
            with_cookie(Response::json(200, &Login { token, user }), &cookie)
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
