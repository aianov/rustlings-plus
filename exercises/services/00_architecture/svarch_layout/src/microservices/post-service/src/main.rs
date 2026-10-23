use contracts::{PostId, UserId};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, PartialEq)]
struct Post {
    id: PostId,
    author: UserId,
    text: String,
}

#[derive(Deserialize)]
struct CreatePostDto {
    author: u64,
    text: String,
}

#[derive(Serialize)]
struct PostDto {
    id: u64,
    author: u64,
    text: String,
}

struct Request {
    method: String,
    path: String,
    body: String,
}

struct Response {
    status: u16,
    body: String,
}

fn error(status: u16, code: &str) -> Response {
    Response { status, body: format!(r#"{{"error":"{code}"}}"#) }
}

fn handle(posts: &Arc<Mutex<Vec<Post>>>, req: Request) -> Response {
    let segments: Vec<&str> = req.path.trim_matches('/').split('/').collect();
    match (req.method.as_str(), segments.as_slice()) {
        ("POST", ["posts"]) => {
            let Ok(dto) = serde_json::from_str::<CreatePostDto>(&req.body) else {
                return error(400, "bad_json");
            };
            let text = dto.text.trim();
            if text.is_empty() {
                return error(422, "empty_text");
            }
            if text.chars().count() > 280 {
                return error(422, "text_too_long");
            }
            let mut posts = posts.lock().unwrap();
            let post = Post {
                id: PostId(posts.len() as u64 + 1),
                author: UserId(dto.author),
                text: text.to_string(),
            };
            posts.push(post.clone());
            let out = PostDto { id: post.id.0, author: post.author.0, text: post.text };
            Response { status: 201, body: serde_json::to_string(&out).unwrap() }
        }
        ("GET", ["posts", raw_id]) => {
            let Ok(id) = raw_id.parse::<u64>() else {
                return error(400, "bad_id");
            };
            let posts = posts.lock().unwrap();
            match posts.iter().find(|p| p.id == PostId(id)) {
                Some(p) => {
                    let out = PostDto { id: p.id.0, author: p.author.0, text: p.text.clone() };
                    Response { status: 200, body: serde_json::to_string(&out).unwrap() }
                }
                None => error(404, "post_not_found"),
            }
        }
        _ => error(404, "not_found"),
    }
}

fn main() {
    let posts = Arc::new(Mutex::new(Vec::new()));
    let req = Request {
        method: "POST".to_string(),
        path: "/posts".to_string(),
        body: r#"{"author":1,"text":"привет"}"#.to_string(),
    };
    let resp = handle(&posts, req);
    println!("{} {}", resp.status, resp.body);
}
