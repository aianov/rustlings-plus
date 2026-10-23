use std::sync::Arc;

use contracts::{Credentials, ErrorBody, Login, Outcome, PostId, PostView, UserView, Username};
use http_engine::{Request, Response, Router, with_state};

use crate::server::Shared;

pub(crate) fn router(shared: Arc<Shared>) -> Router {
    Router::new()
        .post("/auth/register", with_state(Arc::clone(&shared), register))
        .post("/auth/login", with_state(Arc::clone(&shared), login))
        .get("/users/:username", with_state(Arc::clone(&shared), user))
        .get("/posts/:id", with_state(shared, post))
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

fn failure(body: ErrorBody) -> Response {
    let status = status_of(&body.code);
    Response::json(status, &Outcome::<()>::Error(body))
}

fn fail(code: &str, message: &str) -> Response {
    failure(ErrorBody {
        code: code.to_string(),
        message: message.to_string(),
    })
}

async fn register(shared: Arc<Shared>, request: Request) -> Response {
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

async fn login(shared: Arc<Shared>, request: Request) -> Response {
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
        Ok(token) => Response::json(200, &Login { token, user }),
        Err(..) => fail("internal", "генератор случайных чисел недоступен"),
    }
}

async fn user(shared: Arc<Shared>, request: Request) -> Response {
    let raw = request.param("username").unwrap_or_default();
    let Ok(name) = Username::try_from(raw) else {
        return fail("bad_data", &format!("«{raw}» не ник"));
    };
    match shared.app.find_user(name.as_ref()).await {
        Some(found) => Response::json(
            200,
            &UserView {
                id: found.id(),
                username: found.name().to_string(),
            },
        ),
        None => fail("not_found", &format!("пользователя @{} нет", name.as_ref())),
    }
}

fn post_id(raw: &str) -> Option<PostId> {
    match raw.parse::<u64>() {
        Ok(number) => Some(PostId(number)),
        Err(..) => raw.parse().ok(),
    }
}

async fn post(shared: Arc<Shared>, request: Request) -> Response {
    let raw = request.param("id").unwrap_or_default();
    let Some(id) = post_id(raw) else {
        return fail("bad_data", &format!("«{raw}» не id поста"));
    };
    match shared.app.public_post(id).await {
        Some(item) => Response::json(
            200,
            &PostView {
                id: item.id,
                author: item.author,
                author_name: item.author_name.to_string(),
                text: item.text.to_string(),
                likes: item.likes,
            },
        ),
        None => fail("not_found", &format!("поста {id} нет")),
    }
}

async fn health(_: Arc<()>, _: Request) -> Response {
    Response::text(200, "ok")
}
