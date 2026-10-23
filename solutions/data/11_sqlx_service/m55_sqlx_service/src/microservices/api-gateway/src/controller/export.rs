use std::sync::Arc;

use contracts::ErrorCode;
use http_engine::{Request, Response};
use proto::social::v1::ExportRequest;
use tokio::sync::mpsc;

use super::http::{actor, fail, failure, stamp};
use crate::bootstrap::Shared;
use crate::client::failure as explained;
use crate::dto::post_view;

const POSTS: &str = "сервис постов";

pub(crate) const EXPORT_PATH: &str = "/users/me/export";

pub(crate) async fn export(shared: Arc<Shared>, request: Request) -> Response {
    if actor(&request).is_none() {
        return fail(
            ErrorCode::Unauthorized,
            "нужен заголовок Authorization: Bearer <токен>",
        );
    }
    let stamp = stamp(&shared, &request);
    let opened = shared
        .upstreams
        .posts(&stamp)
        .export_posts(ExportRequest {})
        .await;
    let mut posts = match opened {
        Ok(stream) => stream.into_inner(),
        Err(status) => return failure(explained(&status, POSTS, stamp.request_id())),
    };
    let (tx, rx) = mpsc::channel(16);
    tokio::spawn(async move {
        while let Ok(Some(post)) = posts.message().await {
            let Ok(line) = contracts::encode(&post_view(post)) else {
                continue;
            };
            let mut line = line.into_bytes();
            line.push(b'\n');
            if tx.send(line).await.is_err() {
                return;
            }
        }
    });
    Response::streaming(200, "application/x-ndjson", rx)
}
