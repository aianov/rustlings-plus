use std::sync::Arc;

use contracts::{UserView, Username};
use http_engine::{Request, Response, etag, etag_matches};

use super::{actor, fail};
use crate::state::Shared;

fn cached(request: &Request, user: &UserView) -> Response {
    let fresh = Response::json(200, user);
    let tag = etag(&fresh.body);
    let seen = request
        .headers
        .get("if-none-match")
        .is_some_and(|value| etag_matches(value, &tag));
    let mut response = if seen { Response::new(304) } else { fresh };
    response.headers.append("ETag", &tag);
    response.headers.append("Cache-Control", "no-cache");
    response
}

pub(crate) async fn me(shared: Arc<Shared>, request: Request) -> Response {
    let Some(id) = actor(&request) else {
        return fail(
            "unauthorized",
            "нужен заголовок Authorization: Bearer <токен>",
        );
    };
    match shared.app.user(id).await {
        Some(found) => {
            let user = UserView {
                id: found.id(),
                username: found.name().to_string(),
            };
            cached(&request, &user)
        }
        None => fail("not_found", &format!("пользователя {id} больше нет")),
    }
}

pub(crate) async fn user(shared: Arc<Shared>, request: Request) -> Response {
    let raw = request.param("username").unwrap_or_default();
    let Ok(name) = Username::try_from(raw) else {
        return fail("bad_data", &format!("«{raw}» не ник"));
    };
    match shared.app.find_user(name.as_ref()).await {
        Some(found) => {
            let user = UserView {
                id: found.id(),
                username: found.name().to_string(),
            };
            cached(&request, &user)
        }
        None => fail("not_found", &format!("пользователя @{} нет", name.as_ref())),
    }
}
