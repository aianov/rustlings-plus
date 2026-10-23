use std::sync::Arc;

use contracts::UserId;
use http_engine::{Request, Response, etag, serve_bytes};
use proto::social::v1::{AvatarRequest, DeleteAvatarRequest, PutAvatarRequest};

use super::{actor, fail, failure, stamp};
use crate::state::Shared;
use crate::upstream::failure as explained;

const USERS: &str = "сервис профилей";

fn user_id(raw: &str) -> Option<UserId> {
    match raw.parse::<u64>() {
        Ok(number) => Some(UserId(number)),
        Err(..) => raw.parse().ok(),
    }
}

fn signed_in(request: &Request) -> Result<UserId, Response> {
    actor(request).ok_or_else(|| {
        fail(
            "unauthorized",
            "нужен заголовок Authorization: Bearer <токен>",
        )
    })
}

pub(crate) async fn upload(shared: Arc<Shared>, request: Request) -> Response {
    let owner = match signed_in(&request) {
        Ok(owner) => owner,
        Err(response) => return response,
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
    let stamp = stamp(&shared, &request).user(owner.0);
    let ask = PutAvatarRequest {
        content_type: content_type.to_string(),
        data: request.body,
    };
    match shared.upstreams.users(&stamp).put_avatar(ask).await {
        Ok(stored) => {
            let created = stored.into_inner().created;
            let mut response = Response::new(if created { 201 } else { 204 });
            response.headers.append("ETag", &tag);
            response
        }
        Err(status) => failure(explained(&status, USERS, stamp.request_id())),
    }
}

pub(crate) async fn remove(shared: Arc<Shared>, request: Request) -> Response {
    let owner = match signed_in(&request) {
        Ok(owner) => owner,
        Err(response) => return response,
    };
    let stamp = stamp(&shared, &request).user(owner.0);
    match shared
        .upstreams
        .users(&stamp)
        .delete_avatar(DeleteAvatarRequest {})
        .await
    {
        Ok(removed) if removed.get_ref().changed => Response::new(204),
        Ok(..) => fail("not_found", "аватара нет"),
        Err(status) => failure(explained(&status, USERS, stamp.request_id())),
    }
}

pub(crate) async fn download(shared: Arc<Shared>, request: Request) -> Response {
    let raw = request.param("id").unwrap_or_default();
    let Some(id) = user_id(raw) else {
        return fail("bad_data", &format!("«{raw}» не id пользователя"));
    };
    let stamp = stamp(&shared, &request);
    let ask = AvatarRequest { user_id: id.0 };
    match shared.upstreams.users(&stamp).get_avatar(ask).await {
        Ok(avatar) => {
            let avatar = avatar.into_inner();
            serve_bytes(&request, &avatar.content_type, &avatar.data)
        }
        Err(status) => failure(explained(&status, USERS, stamp.request_id())),
    }
}
