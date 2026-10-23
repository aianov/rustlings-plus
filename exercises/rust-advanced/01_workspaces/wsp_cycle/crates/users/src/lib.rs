use posts::Post;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UserId(pub u64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct User {
    pub id: UserId,
    pub name: String,
}

impl User {
    pub fn new(id: u64, name: &str) -> User {
        User { id: UserId(id), name: name.to_string() }
    }
}

pub fn karma(user: &User, posts: &[Post]) -> u32 {
    posts.iter().filter(|p| p.author == user.id).count() as u32 * 10
}
