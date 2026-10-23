use std::error::Error as StdError;
use std::fmt;

use journal::JournalError;

use super::directory::ProfileError;

#[derive(Debug)]
pub enum AuthError {
    Profile(ProfileError),
    UnknownName(String),
    Duplicate(String),
    Storage(JournalError),
    Random(getrandom::Error),
}

impl AuthError {
    pub fn code(&self) -> &str {
        match self {
            AuthError::Profile(ProfileError::Refused { code, .. }) => code,
            AuthError::Profile(ProfileError::Unavailable) => "unavailable",
            AuthError::UnknownName(..) => "unauthorized",
            AuthError::Profile(ProfileError::Failed(..))
            | AuthError::Duplicate(..)
            | AuthError::Storage(..)
            | AuthError::Random(..) => "internal",
        }
    }
}

impl fmt::Display for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AuthError::Profile(error) => write!(f, "{error}"),
            AuthError::UnknownName(name) => {
                write!(f, "пользователя @{name} нет, сначала /auth/register")
            }
            AuthError::Duplicate(name) => {
                write!(
                    f,
                    "учётная запись @{name} уже есть, а профиль только что создан"
                )
            }
            AuthError::Storage(..) => write!(f, "учётная запись не сохранилась, попробуй ещё раз"),
            AuthError::Random(..) => write!(f, "генератор случайных чисел недоступен"),
        }
    }
}

impl StdError for AuthError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            AuthError::Storage(error) => Some(error),
            _ => None,
        }
    }
}
