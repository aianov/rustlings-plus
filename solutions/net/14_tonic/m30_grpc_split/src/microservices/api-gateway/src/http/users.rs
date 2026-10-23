use std::sync::Arc;

use contracts::{UserId, UserView, Username};
use http_engine::{Request, Response, etag, etag_matches};
use proto::social::v1::{FindProfileRequest, GetProfileRequest, Profile};
use proto::{new_request_id, traced};

use super::{actor, fail, failure};
use crate::state::Shared;
use crate::upstream::failure as explained;

const USERS: &str = "сервис профилей";

fn cached(request: &Request, profile: Profile) -> Response {
    let user = UserView {
        id: UserId(profile.id),
        username: profile.username,
    };
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
            "unauthorized",
            "нужен заголовок Authorization: Bearer <токен>",
        );
    };
    let rid = new_request_id();
    let ask = traced(GetProfileRequest { user_id: id.0 }, &rid);
    match shared.upstreams.users().get_profile(ask).await {
        Ok(profile) => cached(&request, profile.into_inner()),
        Err(status) => failure(explained(&status, USERS, &rid)),
    }
}

pub(crate) async fn user(shared: Arc<Shared>, request: Request) -> Response {
    let raw = request.param("username").unwrap_or_default();
    let Ok(name) = Username::try_from(raw) else {
        return fail("bad_data", &format!("«{raw}» не ник"));
    };
    let rid = new_request_id();
    let ask = FindProfileRequest {
        username: name.as_ref().to_string(),
    };
    match shared
        .upstreams
        .users()
        .find_profile(traced(ask, &rid))
        .await
    {
        Ok(profile) => cached(&request, profile.into_inner()),
        Err(status) => failure(explained(&status, USERS, &rid)),
    }
}
