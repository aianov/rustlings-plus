use std::sync::Arc;

use contracts::{ErrorBody, ErrorCode, Outcome, UserId};
use files::{LinkError, safe_headers};
use http_engine::{Request, Response, etag, serve_bytes};
use proto::social::v1::{AvatarChunk, DeleteAvatarRequest, FileRequest};

use super::http::{actor, fail, failure, stamp};
use crate::bootstrap::Shared;
use crate::client::failure as explained;

const USERS: &str = "сервис профилей";

pub(crate) const CHUNK: usize = 16 * 1024;

fn signed_in(request: &Request) -> Result<UserId, Response> {
    actor(request).ok_or_else(|| {
        fail(
            ErrorCode::Unauthorized,
            "нужен заголовок Authorization: Bearer <токен>",
        )
    })
}

fn declared(request: &Request) -> Option<usize> {
    request.headers.get("content-length")?.trim().parse().ok()
}

pub(crate) async fn upload(shared: Arc<Shared>, request: Request) -> Response {
    let owner = match signed_in(&request) {
        Ok(owner) => owner,
        Err(response) => return response,
    };
    let limit = shared.config.avatar_max;
    let size = declared(&request).unwrap_or(request.body.len());
    if size > limit || request.body.len() > limit {
        return fail(ErrorCode::TooLarge, &format!("аватар больше {limit} байт"));
    }
    if request.body.is_empty() {
        return fail(ErrorCode::BadData, "пустое тело: пришли картинку");
    }
    let tag = etag(&request.body);
    let stamp = stamp(&shared, &request)
        .within(shared.config.upload_budget)
        .user(owner.0);
    let chunks: Vec<AvatarChunk> = request
        .body
        .chunks(CHUNK)
        .map(|piece| AvatarChunk {
            data: piece.to_vec(),
        })
        .collect();
    let body = futures::stream::iter(chunks);
    match shared.upstreams.users(&stamp).upload_avatar(body).await {
        Ok(stored) => {
            let stored = stored.into_inner();
            let mut response = Response::new(if stored.created { 201 } else { 204 });
            response.headers.append("ETag", &tag);
            response.headers.append(
                "Location",
                &shared.links.avatar(&stored.key, &stored.content_type),
            );
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
        Ok(..) => fail(ErrorCode::NotFound, "аватара нет"),
        Err(status) => failure(explained(&status, USERS, stamp.request_id())),
    }
}

fn refused(error: LinkError) -> Response {
    let (status, code, message) = match error {
        LinkError::Format => (400, ErrorCode::BadData, "ссылка на файл испорчена"),
        LinkError::Signature => (403, ErrorCode::Forbidden, "подпись ссылки не сходится"),
        LinkError::Expired => (410, ErrorCode::Expired, "срок ссылки вышел"),
    };
    Response::json(status, &Outcome::<()>::Error(ErrorBody::new(code, message)))
}

pub(crate) async fn download(shared: Arc<Shared>, request: Request) -> Response {
    let linked = match shared.links.verify(&request.target) {
        Ok(linked) => linked,
        Err(error) => return refused(error),
    };
    let stamp = stamp(&shared, &request);
    let ask = FileRequest {
        key: linked.key.clone(),
    };
    match shared.upstreams.users(&stamp).get_file(ask).await {
        Ok(file) => {
            let file = file.into_inner();
            let (_, headers) = safe_headers(&file.data, &linked.name);
            let mut kind = String::from("application/octet-stream");
            let mut rest = Vec::new();
            for (name, value) in headers {
                if name == "Content-Type" {
                    kind = value;
                } else {
                    rest.push((name, value));
                }
            }
            let mut response = serve_bytes(&request, &kind, &file.data);
            for (name, value) in rest {
                response.headers.append(name, &value);
            }
            response
        }
        Err(status) => failure(explained(&status, USERS, stamp.request_id())),
    }
}
