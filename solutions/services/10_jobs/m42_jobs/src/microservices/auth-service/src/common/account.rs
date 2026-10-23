use contracts::{Email, Role, Username};
use std::fmt;

use serde::{Deserialize, Serialize};
use service_runtime::Secret;

fn user() -> String {
    Role::User.as_str().to_string()
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    pub user_id: u64,
    pub username: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(default)]
    pub email_verified: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(default = "user")]
    pub role: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub frozen: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub deleted: bool,
}

impl fmt::Debug for Account {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Account")
            .field("user_id", &self.user_id)
            .field("username", &self.username)
            .field("email", &self.email)
            .field("email_verified", &self.email_verified)
            .field("role", &self.role)
            .field("frozen", &self.frozen)
            .field("deleted", &self.deleted)
            .finish_non_exhaustive()
    }
}

impl Account {
    pub fn new(user_id: u64, username: &str) -> Account {
        Account {
            user_id,
            username: username.to_string(),
            email: None,
            email_verified: false,
            password: None,
            role: user(),
            frozen: false,
            deleted: false,
        }
    }

    pub fn role(&self) -> Role {
        Role::parse(&self.role)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Registration {
    pub username: Username,
    pub email: Option<Email>,
    pub password: Secret<String>,
}
