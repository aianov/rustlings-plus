use std::sync::Arc;

use contracts::ErrorCode;
use http_engine::{Request, Response};
use proto::social::v1::{CommentAction, DraftAction, DraftRequest};
use serde::{Deserialize, Serialize};

use super::http::{actor, fail, failure, json, stamp};
use crate::bootstrap::Shared;
use crate::client::failure as explained;

const POSTS: &str = "сервис постов";

#[derive(Debug, Deserialize)]
struct DraftBody {
    text: String,
}

#[derive(Debug, Serialize)]
struct DraftView {
    id: u64,
    text: String,
}

fn number(request: &Request) -> Result<u64, Response> {
    let raw = request.param("id").unwrap_or_default();
    raw.parse()
        .map_err(|_| fail(ErrorCode::BadData, &format!("«{raw}» не номер")))
}

fn signed(request: &Request) -> Result<(), Response> {
    match actor(request) {
        Some(..) => Ok(()),
        None => Err(fail(
            ErrorCode::Unauthorized,
            "нужен заголовок Authorization: Bearer <токен>",
        )),
    }
}

pub(crate) async fn save(shared: Arc<Shared>, request: Request) -> Response {
    if let Err(response) = signed(&request) {
        return response;
    }
    let body: DraftBody = match json(&request) {
        Ok(body) => body,
        Err(response) => return response,
    };
    let stamp = stamp(&shared, &request);
    let ask = DraftRequest { text: body.text };
    match shared.upstreams.posts(&stamp).save_draft(ask).await {
        Ok(reply) => {
            let reply = reply.into_inner();
            let view = DraftView {
                id: reply.draft_id,
                text: reply.text,
            };
            Response::json(201, &view)
        }
        Err(status) => failure(explained(&status, POSTS, stamp.request_id())),
    }
}

pub(crate) async fn read(shared: Arc<Shared>, request: Request) -> Response {
    if let Err(response) = signed(&request) {
        return response;
    }
    let id = match number(&request) {
        Ok(id) => id,
        Err(response) => return response,
    };
    let stamp = stamp(&shared, &request);
    let ask = DraftAction { draft_id: id };
    match shared.upstreams.posts(&stamp).get_draft(ask).await {
        Ok(reply) => {
            let reply = reply.into_inner();
            let view = DraftView {
                id: reply.draft_id,
                text: reply.text,
            };
            Response::json(200, &view)
        }
        Err(status) => failure(explained(&status, POSTS, stamp.request_id())),
    }
}

pub(crate) async fn drop(shared: Arc<Shared>, request: Request) -> Response {
    if let Err(response) = signed(&request) {
        return response;
    }
    let id = match number(&request) {
        Ok(id) => id,
        Err(response) => return response,
    };
    let stamp = stamp(&shared, &request);
    let ask = DraftAction { draft_id: id };
    match shared.upstreams.posts(&stamp).delete_draft(ask).await {
        Ok(..) => Response::new(204),
        Err(status) => failure(explained(&status, POSTS, stamp.request_id())),
    }
}

pub(crate) async fn uncomment(shared: Arc<Shared>, request: Request) -> Response {
    if let Err(response) = signed(&request) {
        return response;
    }
    let raw = request.param("id").unwrap_or_default();
    let digits = raw.strip_prefix("c_").unwrap_or(raw);
    let Ok(id) = digits.parse::<u64>() else {
        return fail(ErrorCode::BadData, &format!("«{raw}» не id комментария"));
    };
    let stamp = stamp(&shared, &request);
    let ask = CommentAction { comment_id: id };
    match shared.upstreams.posts(&stamp).delete_comment(ask).await {
        Ok(reply) if reply.get_ref().changed => Response::new(204),
        Ok(..) => fail(ErrorCode::NotFound, "комментария нет"),
        Err(status) => failure(explained(&status, POSTS, stamp.request_id())),
    }
}
