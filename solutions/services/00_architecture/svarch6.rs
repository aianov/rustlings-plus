use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UserId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PostId(pub u64);

#[derive(Clone, Debug, PartialEq)]
pub struct Post {
    pub id: PostId,
    pub author: UserId,
    pub text: String,
}

#[derive(Debug, PartialEq)]
pub enum PostError {
    Empty,
    TooLong,
}

pub struct HttpRequest {
    pub author: u64,
    pub body: String,
}

#[derive(Debug, PartialEq)]
pub struct HttpResponse {
    pub status: u16,
    pub body: String,
}

#[derive(Deserialize)]
pub struct CreatePostDto {
    pub text: String,
}

#[derive(Serialize)]
pub struct PostDto {
    pub id: u64,
    pub author: u64,
    pub text: String,
}

impl From<Post> for PostDto {
    fn from(post: Post) -> Self {
        PostDto { id: post.id.0, author: post.author.0, text: post.text }
    }
}

pub mod service {
    use super::*;

    #[derive(Default)]
    pub struct PostService {
        last_id: u64,
    }

    impl PostService {
        pub fn create(&mut self, author: UserId, text: &str) -> Result<Post, PostError> {
            let text = text.trim();
            if text.is_empty() {
                return Err(PostError::Empty);
            }
            if text.chars().count() > 280 {
                return Err(PostError::TooLong);
            }
            self.last_id += 1;
            Ok(Post { id: PostId(self.last_id), author, text: text.to_string() })
        }
    }
}

pub mod controller {
    use super::service::PostService;
    use super::*;

    fn error(status: u16, code: &str) -> HttpResponse {
        HttpResponse { status, body: format!(r#"{{"error":"{code}"}}"#) }
    }

    pub fn create(service: &mut PostService, req: &HttpRequest) -> HttpResponse {
        let Ok(dto) = serde_json::from_str::<CreatePostDto>(&req.body) else {
            return error(400, "bad_json");
        };
        match service.create(UserId(req.author), &dto.text) {
            Ok(post) => HttpResponse {
                status: 201,
                body: serde_json::to_string(&PostDto::from(post)).unwrap(),
            },
            Err(PostError::Empty) => error(422, "empty_text"),
            Err(PostError::TooLong) => error(422, "text_too_long"),
        }
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::controller;
    use super::service::PostService;
    use super::*;

    #[test]
    fn service_works_without_http() {
        let mut service = PostService::default();
        let post = service.create(UserId(1), "  привет ").unwrap();
        assert_eq!(
            post,
            Post { id: PostId(1), author: UserId(1), text: "привет".to_string() },
            "service отдаёт доменный Post с обрезанным текстом"
        );
        assert_eq!(service.create(UserId(1), "ещё").unwrap().id, PostId(2), "id растут");
    }

    #[test]
    fn service_reports_rule_violations_as_values() {
        let mut service = PostService::default();
        assert_eq!(service.create(UserId(1), "  "), Err(PostError::Empty), "пустой текст");
        assert_eq!(
            service.create(UserId(1), &"x".repeat(281)),
            Err(PostError::TooLong),
            "слишком длинный текст"
        );
        assert_eq!(
            service.create(UserId(1), "ok").unwrap().id,
            PostId(1),
            "отклонённые посты не расходуют id"
        );
    }

    #[test]
    fn controller_builds_201() {
        let mut service = PostService::default();
        let req = HttpRequest { author: 3, body: r#"{"text":"hi"}"#.to_string() };
        let resp = controller::create(&mut service, &req);
        assert_eq!(resp.status, 201, "успех: 201");
        assert_eq!(
            resp.body,
            r#"{"id":1,"author":3,"text":"hi"}"#,
            "тело ответа это PostDto: поля id, author, text в этом порядке"
        );
    }

    #[test]
    fn controller_maps_errors_to_statuses() {
        let mut service = PostService::default();
        let bad = HttpRequest { author: 1, body: "{oops".to_string() };
        assert_eq!(
            controller::create(&mut service, &bad),
            HttpResponse { status: 400, body: r#"{"error":"bad_json"}"#.to_string() },
            "битый JSON это 400, и это решает контроллер"
        );
        let empty = HttpRequest { author: 1, body: r#"{"text":" "}"#.to_string() };
        assert_eq!(
            controller::create(&mut service, &empty),
            HttpResponse { status: 422, body: r#"{"error":"empty_text"}"#.to_string() },
            "PostError::Empty превращается в 422 empty_text"
        );
        let long = HttpRequest { author: 1, body: format!(r#"{{"text":"{}"}}"#, "x".repeat(300)) };
        assert_eq!(
            controller::create(&mut service, &long),
            HttpResponse { status: 422, body: r#"{"error":"text_too_long"}"#.to_string() },
            "PostError::TooLong превращается в 422 text_too_long"
        );
    }
}
