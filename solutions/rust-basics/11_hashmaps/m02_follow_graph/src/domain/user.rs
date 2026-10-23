#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UserId(pub u64);

#[derive(Debug)]
pub struct User {
    id: UserId,
    name: String,
    bio: String,
}

impl User {
    pub fn new(id: UserId, name: String) -> User {
        User {
            id,
            name,
            bio: String::new(),
        }
    }

    pub fn id(&self) -> UserId {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn bio(&self) -> &str {
        &self.bio
    }

    pub fn rename(&mut self, name: String) {
        self.name = name;
    }

    pub fn set_bio(&mut self, bio: String) {
        self.bio = bio;
    }
}

pub fn normalize_name(name: &str) -> String {
    name.trim().to_lowercase()
}
