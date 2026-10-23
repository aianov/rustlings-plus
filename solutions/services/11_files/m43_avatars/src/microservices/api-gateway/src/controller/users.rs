use std::sync::Arc;

use contracts::{ErrorCode, ProfilePatch, Username};
use http_engine::{Request, Response, etag, etag_matches};
use proto::social::v1::{FindProfileRequest, GetProfileRequest, Profile};

use super::http::{actor, fail, failure, json, stamp};
use crate::bootstrap::Shared;
use crate::client::failure as explained;
use crate::dto;

const USERS: &str = "сервис профилей";

fn cached(shared: &Shared, request: &Request, profile: Profile) -> Response {
    let user = dto::user_view(profile, &shared.links);
    let fresh = Response::json(200, &user);
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
            ErrorCode::Unauthorized,
            "нужен заголовок Authorization: Bearer <токен>",
        );
    };
    let stamp = stamp(&shared, &request);
    let ask = GetProfileRequest { user_id: id.0 };
    match shared.upstreams.users(&stamp).get_profile(ask).await {
        Ok(profile) => cached(&shared, &request, profile.into_inner()),
        Err(status) => failure(explained(&status, USERS, stamp.request_id())),
    }
}

pub(crate) async fn user(shared: Arc<Shared>, request: Request) -> Response {
    let raw = request.param("username").unwrap_or_default();
    let Ok(name) = Username::try_from(raw) else {
        return fail(ErrorCode::BadData, &format!("«{raw}» не ник"));
    };
    let stamp = stamp(&shared, &request);
    let ask = FindProfileRequest {
        username: name.as_ref().to_string(),
    };
    match shared.upstreams.users(&stamp).find_profile(ask).await {
        Ok(profile) => cached(&shared, &request, profile.into_inner()),
        Err(status) => failure(explained(&status, USERS, stamp.request_id())),
    }
}

pub(crate) async fn update(shared: Arc<Shared>, request: Request) -> Response {
    if actor(&request).is_none() {
        return fail(
            ErrorCode::Unauthorized,
            "нужен заголовок Authorization: Bearer <токен>",
        );
    }
    let patch: ProfilePatch = match json(&request) {
        Ok(patch) => patch,
        Err(response) => return response,
    };
    let stamp = stamp(&shared, &request);
    let ask = dto::profile_update(&patch);
    match shared.upstreams.users(&stamp).update_profile(ask).await {
        Ok(profile) => Response::json(200, &dto::user_view(profile.into_inner(), &shared.links)),
        Err(status) => failure(explained(&status, USERS, stamp.request_id())),
    }
}
