use contracts::UserId;

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
