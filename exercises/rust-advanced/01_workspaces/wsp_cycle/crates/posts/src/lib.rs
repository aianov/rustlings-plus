use users::{User, UserId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Post {
    pub author: UserId,
    pub text: String,
}

impl Post {
    pub fn new(author: &User, text: &str) -> Post {
        Post { author: author.id, text: text.to_string() }
    }
}
