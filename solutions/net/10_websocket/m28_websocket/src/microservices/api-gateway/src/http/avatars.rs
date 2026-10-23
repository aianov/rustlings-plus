use std::sync::Arc;

use contracts::UserId;
use http_engine::{Request, Response, etag, serve_bytes};

use super::{actor, fail};
use crate::media::Avatar;
use crate::state::Shared;

fn user_id(raw: &str) -> Option<UserId> {
    match raw.parse::<u64>() {
        Ok(number) => Some(UserId(number)),
        Err(..) => raw.parse().ok(),
    }
}

pub(crate) async fn upload(shared: Arc<Shared>, request: Request) -> Response {
    let Some(owner) = actor(&request) else {
        return fail(
            "unauthorized",
            "нужен заголовок Authorization: Bearer <токен>",
        );
    };
    let limit = shared.config.avatar_max;
    if request.body.len() > limit {
        return fail("too_large", &format!("аватар больше {limit} байт"));
    }
    let content_type = request.headers.get("content-type").unwrap_or_default();
    if !content_type.starts_with("image/") {
        return fail("unsupported", "аватар это картинка: Content-Type image/...");
    }
    let tag = etag(&request.body);
    let avatar = Avatar {
        content_type: content_type.to_string(),
        bytes: Arc::new(request.body),
    };
    let created = shared.avatars.put(owner, avatar);
    let mut response = Response::new(if created { 201 } else { 204 });
    response.headers.append("ETag", &tag);
    response
}

pub(crate) async fn remove(shared: Arc<Shared>, request: Request) -> Response {
    let Some(owner) = actor(&request) else {
        return fail(
            "unauthorized",
            "нужен заголовок Authorization: Bearer <токен>",
        );
    };
    if shared.avatars.remove(owner) {
        Response::new(204)
    } else {
        fail("not_found", "аватара нет")
    }
}

pub(crate) async fn download(shared: Arc<Shared>, request: Request) -> Response {
    let raw = request.param("id").unwrap_or_default();
    let Some(id) = user_id(raw) else {
        return fail("bad_data", &format!("«{raw}» не id пользователя"));
    };
    match shared.avatars.get(id) {
        Some(avatar) => serve_bytes(&request, &avatar.content_type, &avatar.bytes),
        None => fail("not_found", &format!("у {id} нет аватара")),
    }
}
