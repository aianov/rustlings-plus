use std::borrow::Cow;
use std::error::Error;
use std::fmt;

use serde::{Deserialize, Serialize};

pub const MAX_NAME_LEN: usize = 20;

pub fn normalize_name(name: &str) -> Cow<'_, str> {
    let trimmed = name.trim();
    if trimmed.bytes().any(|b| b.is_ascii_uppercase()) {
        Cow::Owned(trimmed.to_ascii_lowercase())
    } else {
        Cow::Borrowed(trimmed)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameError {
    Empty,
    TooLong,
    BadChar(char),
}

impl fmt::Display for NameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NameError::Empty => write!(f, "ник не может быть пустым"),
            NameError::TooLong => write!(f, "ник длиннее {MAX_NAME_LEN} символов"),
            NameError::BadChar(c) => {
                write!(f, "в нике можно только a-z, 0-9 и _, а {c:?} нельзя")
            }
        }
    }
}

impl Error for NameError {}

fn allowed(c: char) -> bool {
    c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub struct Username(String);

impl TryFrom<&str> for Username {
    type Error = NameError;

    fn try_from(raw: &str) -> Result<Username, NameError> {
        let name = normalize_name(raw);
        if name.is_empty() {
            return Err(NameError::Empty);
        }
        if let Some(bad) = name.chars().find(|c| !allowed(*c)) {
            return Err(NameError::BadChar(bad));
        }
        if name.len() > MAX_NAME_LEN {
            return Err(NameError::TooLong);
        }
        Ok(Username(name.into_owned()))
    }
}

impl TryFrom<String> for Username {
    type Error = NameError;

    fn try_from(raw: String) -> Result<Username, NameError> {
        Username::try_from(raw.as_str())
    }
}

impl AsRef<str> for Username {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<Username> for String {
    fn from(name: Username) -> String {
        name.0
    }
}

impl fmt::Display for Username {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "@{}", self.0)
    }
}
