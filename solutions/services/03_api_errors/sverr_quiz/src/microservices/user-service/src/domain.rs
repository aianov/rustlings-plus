use contracts::api::User;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum UserError {
    NotFound(u64),
    NameTaken,
    Storage(String),
}

pub struct UserStore {
    users: HashMap<u64, User>,
    broken: Option<String>,
}

impl UserStore {
    pub fn with_users(users: Vec<User>) -> UserStore {
        UserStore { users: users.into_iter().map(|u| (u.id, u)).collect(), broken: None }
    }

    pub fn broken(reason: &str) -> UserStore {
        UserStore { users: HashMap::new(), broken: Some(reason.to_string()) }
    }

    pub fn get(&self, id: u64) -> Result<User, UserError> {
        if let Some(reason) = &self.broken {
            return Err(UserError::Storage(reason.clone()));
        }
        self.users.get(&id).cloned().ok_or(UserError::NotFound(id))
    }
}
