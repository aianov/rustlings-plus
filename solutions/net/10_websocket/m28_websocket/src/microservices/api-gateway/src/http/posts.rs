use std::sync::Arc;

use contracts::{PostId, PostView};
use http_engine::{Request, Response};

use super::fail;
use crate::state::Shared;

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
