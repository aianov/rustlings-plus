use std::borrow::Cow;
use std::error::Error;
use std::fmt;

use serde::{Deserialize, Serialize};
use validation::Violation;

pub const MAX_NAME_LEN: usize = 20;

pub const MIN_NEW_NAME_LEN: usize = 3;

/// ```
/// use std::borrow::Cow;
///
/// use contracts::normalize_name;
///
/// assert_eq!(normalize_name("  Alice "), "alice");
/// assert!(matches!(normalize_name("bob"), Cow::Borrowed("bob")));
/// ```
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

impl From<NameError> for Violation {
    fn from(error: NameError) -> Violation {
        let code = match error {
            NameError::Empty => "blank",
            NameError::TooLong => "too_long",
            NameError::BadChar(..) => "invalid_char",
        };
        Violation::new(code, error.to_string())
    }
}

fn allowed(c: char) -> bool {
    c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'
}

/// ```
/// use contracts::{NameError, Username};
///
/// let name = Username::try_from("Alice_1")?;
/// assert_eq!(name.as_ref(), "alice_1");
/// assert_eq!(name.to_string(), "@alice_1");
/// assert_eq!(Username::try_from("   "), Err(NameError::Empty));
/// assert_eq!(Username::try_from("аlice"), Err(NameError::BadChar('а')));
/// # Ok::<(), NameError>(())
/// ```
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
