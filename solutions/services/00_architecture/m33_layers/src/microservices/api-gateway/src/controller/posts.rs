use std::sync::Arc;

use contracts::PostId;
use http_engine::{Request, Response};
use proto::social::v1::GetPostRequest;

use super::http::{fail, failure, stamp};
use crate::bootstrap::Shared;
use crate::client::failure as explained;
use crate::dto::post_view;

fn post_id(raw: &str) -> Option<PostId> {
    match raw.parse::<u64>() {
        Ok(number) => Some(PostId(number)),
        Err(..) => raw.parse().ok(),
    }
}

pub(crate) async fn post(shared: Arc<Shared>, request: Request) -> Response {
    let raw = request.param("id").unwrap_or_default();
    let Some(id) = post_id(raw) else {
        return fail("bad_data", &format!("«{raw}» не id поста"));
    };
    let stamp = stamp(&shared, &request);
    let ask = GetPostRequest { post_id: id.0 };
    match shared.upstreams.posts(&stamp).get_post(ask).await {
        Ok(post) => Response::json(200, &post_view(post.into_inner())),
        Err(status) => failure(explained(&status, "сервис постов", stamp.request_id())),
    }
}
