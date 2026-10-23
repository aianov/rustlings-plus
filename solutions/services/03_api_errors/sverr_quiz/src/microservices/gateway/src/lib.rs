use contracts::api::{PostApi, UserApi};
use contracts::errors::{ApiError, Code, HttpReply, from_status, http_reply};
use serde_json::{Value, json};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Mutex};

pub struct HttpRequest {
    pub id: String,
    pub method: String,
    pub path: String,
    pub body: Vec<u8>,
}

pub struct Gateway {
    users: Arc<dyn UserApi>,
    posts: Arc<dyn PostApi>,
    log: Arc<Mutex<Vec<String>>>,
}

pub fn parse_create_post(body: &[u8]) -> Result<(u64, String), ApiError> {
    let value: Value = serde_json::from_slice(body).map_err(|_| ApiError::new(Code::Malformed))?;
    let author_id = value.get("author_id").and_then(Value::as_u64);
    let text = value.get("text").and_then(Value::as_str);
    match (author_id, text) {
        (Some(author_id), Some(text)) if !text.trim().is_empty() => Ok((author_id, text.to_string())),
        _ => {
            let mut err = ApiError::new(Code::ValidationFailed);
            if author_id.is_none() {
                err = err.with_detail("author_id", "required");
            }
            if text.is_none_or(|t| t.trim().is_empty()) {
                err = err.with_detail("text", "required");
            }
            Err(err)
        }
    }
}

impl Gateway {
    pub fn new(users: Arc<dyn UserApi>, posts: Arc<dyn PostApi>, log: Arc<Mutex<Vec<String>>>) -> Gateway {
        Gateway { users, posts, log }
    }

    pub fn handle(&self, request: HttpRequest) -> HttpReply {
        let id = request.id.clone();
        match catch_unwind(AssertUnwindSafe(|| self.route(&request))) {
            Ok(reply) => reply,
            Err(payload) => {
                let text = payload
                    .downcast_ref::<&str>()
                    .map(|s| s.to_string())
                    .or_else(|| payload.downcast_ref::<String>().cloned())
                    .unwrap_or_else(|| "паника без текста".to_string());
                self.log.lock().unwrap_or_else(|p| p.into_inner()).push(format!("request_id={id} panic: {text}"));
                http_reply(&ApiError::new(Code::Internal), &id)
            }
        }
    }

    fn route(&self, request: &HttpRequest) -> HttpReply {
        let id = request.id.as_str();
        let result = match (request.method.as_str(), request.path.as_str()) {
            ("GET", path) if path.starts_with("/users/") => match path["/users/".len()..].parse::<u64>() {
                Ok(user_id) => self
                    .users
                    .get(user_id, id)
                    .map(|user| json!({"id": user.id, "name": user.name}))
                    .map_err(|status| self.upstream(id, &status)),
                Err(_) => Err(ApiError::new(Code::Malformed)),
            },
            ("POST", "/posts") => parse_create_post(&request.body).and_then(|(author_id, text)| {
                self.posts
                    .create(author_id, &text, id)
                    .map(|post| json!({"id": post.id, "author_id": post.author_id, "text": post.text}))
                    .map_err(|status| self.upstream(id, &status))
            }),
            _ => Err(ApiError::new(Code::Malformed)),
        };
        match result {
            Ok(body) => HttpReply {
                status: 200,
                headers: vec![("content-type".to_string(), "application/json".to_string())],
                body: body.to_string(),
            },
            Err(err) => {
                if !err.code.is_client_fault() {
                    self.log.lock().unwrap_or_else(|p| p.into_inner()).push(format!("request_id={id} code={}", err.code.as_str()));
                }
                http_reply(&err, id)
            }
        }
    }

    fn upstream(&self, request_id: &str, status: &contracts::errors::GrpcStatus) -> ApiError {
        self.log
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push(format!("request_id={request_id} upstream={:?} message={}", status.code, status.message));
        from_status(status)
    }
}
