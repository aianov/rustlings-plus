use crate::dto::{CreatePostDto, PostDto};
use crate::service::{PostError, PostService};
use contracts::{PostId, UserId};

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

fn json(status: u16, value: &PostDto) -> Response {
    Response { status, body: serde_json::to_string(value).unwrap() }
}

pub struct Controller {
    service: PostService,
}

impl Controller {
    pub fn new(service: PostService) -> Self {
        Self { service }
    }

    pub fn handle(&self, req: Request) -> Response {
        let segments: Vec<&str> = req.path.trim_matches('/').split('/').collect();
        match (req.method.as_str(), segments.as_slice()) {
            ("POST", ["posts"]) => self.create(&req.body),
            ("GET", ["posts", raw_id]) => self.get(raw_id),
            _ => error(404, "not_found"),
        }
    }

    fn create(&self, body: &str) -> Response {
        let Ok(dto) = serde_json::from_str::<CreatePostDto>(body) else {
            return error(400, "bad_json");
        };
        match self.service.create(UserId(dto.author), &dto.text) {
            Ok(post) => json(201, &post.into()),
            Err(PostError::Empty) => error(422, "empty_text"),
            Err(PostError::TooLong) => error(422, "text_too_long"),
        }
    }

    fn get(&self, raw_id: &str) -> Response {
        let Ok(id) = raw_id.parse::<u64>() else {
            return error(400, "bad_id");
        };
        match self.service.get(PostId(id)) {
            Some(post) => json(200, &post.into()),
            None => error(404, "post_not_found"),
        }
    }
}
