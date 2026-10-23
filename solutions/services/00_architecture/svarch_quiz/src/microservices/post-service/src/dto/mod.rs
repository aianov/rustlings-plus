use crate::common::Post;
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct CreatePostDto {
    pub author: u64,
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
