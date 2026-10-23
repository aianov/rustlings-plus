use contracts::UserId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Post {
    pub author: UserId,
    pub text: String,
}

impl Post {
    pub fn new(author: UserId, text: &str) -> Post {
        Post { author, text: text.to_string() }
    }
}
