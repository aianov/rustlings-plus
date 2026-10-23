use contracts::{Email, Username};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    pub user_id: u64,
    pub username: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Registration {
    pub username: Username,
    pub email: Option<Email>,
}

impl Registration {
    pub fn named(username: Username) -> Registration {
        Registration {
            username,
            email: None,
        }
    }
}
