use std::fmt;

use serde::{Deserialize, Serialize};

use crate::api::UserView;
use crate::username::Username;

pub const TOKEN_LEN: usize = 32;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Credentials {
    pub username: Username,
    pub password: String,
}

impl fmt::Debug for Credentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Credentials")
            .field("username", &self.username)
            .field("password", &"***")
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Login {
    pub token: String,
    pub user: UserView,
    #[serde(default)]
    pub expires_in: u64,
}

/// ```
/// use contracts::is_token;
///
/// assert!(is_token("0123456789abcdef0123456789abcdef"));
/// assert!(!is_token("0123456789ABCDEF0123456789ABCDEF"));
/// assert!(!is_token("short"));
/// ```
pub fn is_token(raw: &str) -> bool {
    raw.len() == TOKEN_LEN
        && raw
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
