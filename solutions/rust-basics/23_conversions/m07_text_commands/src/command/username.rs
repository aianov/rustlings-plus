use std::fmt;

use crate::domain::{DomainError, validate_name};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Username(String);

impl TryFrom<&str> for Username {
    type Error = DomainError;

    fn try_from(raw: &str) -> Result<Username, DomainError> {
        validate_name(raw).map(Username)
    }
}

impl AsRef<str> for Username {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Username {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "@{}", self.0)
    }
}
