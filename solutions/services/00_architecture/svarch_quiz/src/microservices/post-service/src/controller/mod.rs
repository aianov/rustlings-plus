use crate::dto::{CreatePostDto, PostDto};
use crate::service::{PostError, PostService};
use contracts::{PostId, UserId};
use serde::Serialize;

pub struct Request {
    pub method: String,
    pub path: String,
    pub body: String,
}

pub struct Response {
    pub status: u16,
    pub body: String,
}

fn error(status: u16, code: &str) -> Response {
    Response { status, body: format!(r#"{{"error":"{code}"}}"#) }
}

fn json<T: Serialize>(status: u16, value: &T) -> Response {
    Response { status, body: serde_json::to_string(value).unwrap() }
}

pub struct Controller {
    service: PostService,
}

impl Controller {
    pub fn new(service: PostService) -> Self {
        Self { service }
    }

    pub async fn handle(&self, req: Request) -> Response {
        let segments: Vec<&str> = req.path.trim_matches('/').split('/').collect();
        match (req.method.as_str(), segments.as_slice()) {
            ("POST", ["posts"]) => self.create(&req.body).await,
            ("GET", ["posts", raw_id]) => self.get(raw_id).await,
            ("GET", ["users", raw_id, "posts"]) => self.by_author(raw_id).await,
            _ => error(404, "not_found"),
        }
    }

    async fn create(&self, body: &str) -> Response {
        let Ok(dto) = serde_json::from_str::<CreatePostDto>(body) else {
            return error(400, "bad_json");
        };
        match self.service.create(UserId(dto.author), &dto.text).await {
            Ok(post) => json(201, &PostDto::from(post)),
            Err(PostError::Empty) => error(422, "empty_text"),
            Err(PostError::TooLong) => error(422, "text_too_long"),
            Err(PostError::AuthorNotFound) => error(404, "author_not_found"),
            Err(PostError::AuthorCheckUnavailable) => error(503, "users_unavailable"),
            Err(PostError::Duplicate) => error(409, "duplicate_post"),
            Err(PostError::Storage) => error(503, "storage_unavailable"),
        }
    }

    async fn get(&self, raw_id: &str) -> Response {
        let Ok(id) = raw_id.parse::<u64>() else {
            return error(400, "bad_id");
        };
        match self.service.get(PostId(id)).await {
            Some(post) => json(200, &PostDto::from(post)),
            None => error(404, "post_not_found"),
        }
    }

    async fn by_author(&self, raw_id: &str) -> Response {
        let Ok(id) = raw_id.parse::<u64>() else {
            return error(400, "bad_id");
        };
        let posts: Vec<PostDto> =
            self.service.by_author(UserId(id)).await.into_iter().map(PostDto::from).collect();
        json(200, &posts)
    }
}
