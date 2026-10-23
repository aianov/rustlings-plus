use std::error::Error as StdError;
use std::fmt;

use contracts::{ErrorBody, ErrorCode};
use journal::JournalError;

use super::directory::ProfileError;
use super::password::PasswordError;
use crate::repository::RefreshError;

#[derive(Debug)]
pub enum AuthError {
    Profile(ProfileError),
    Duplicate(String),
    InvalidCredentials,
    TokenInvalid,
    TokenExpired,
    TokenRevoked,
    Refresh(RefreshError),
    OnceInvalid,
    NoEmail,
    Storage(JournalError),
    Password(PasswordError),
    Random,
    Queue(String),
}

impl AuthError {
    pub fn code(&self) -> ErrorCode {
        match self {
            AuthError::Profile(ProfileError::Refused(body)) => {
                body.known().unwrap_or(ErrorCode::Internal)
            }
            AuthError::Profile(ProfileError::Unavailable) | AuthError::Queue(..) => {
                ErrorCode::Unavailable
            }
            AuthError::InvalidCredentials
            | AuthError::TokenInvalid
            | AuthError::TokenRevoked
            | AuthError::Refresh(..) => ErrorCode::Unauthorized,
            AuthError::TokenExpired => ErrorCode::Expired,
            AuthError::OnceInvalid | AuthError::NoEmail => ErrorCode::Rejected,
            AuthError::Profile(ProfileError::Failed(..))
            | AuthError::Duplicate(..)
            | AuthError::Storage(..)
            | AuthError::Password(..)
            | AuthError::Random => ErrorCode::Internal,
        }
    }

    pub fn body(&self) -> ErrorBody {
        match self {
            AuthError::Profile(ProfileError::Refused(body)) if self.code().is_client_fault() => {
                body.clone()
            }
            _ => ErrorBody::new(self.code(), self.to_string()),
        }
    }
}

impl From<JournalError> for AuthError {
    fn from(error: JournalError) -> AuthError {
        AuthError::Storage(error)
    }
}

impl From<PasswordError> for AuthError {
    fn from(error: PasswordError) -> AuthError {
        AuthError::Password(error)
    }
}

impl fmt::Display for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AuthError::Profile(error) => write!(f, "{error}"),
            AuthError::Duplicate(name) => {
                write!(
                    f,
                    "учётная запись @{name} уже есть, а профиль только что создан"
                )
            }
            AuthError::InvalidCredentials => write!(f, "неверный ник или пароль"),
            AuthError::TokenInvalid => write!(f, "токен не принят: войди заново"),
            AuthError::TokenExpired => write!(f, "срок токена вышел: обнови его по refresh"),
            AuthError::TokenRevoked => write!(f, "сессия закрыта: войди заново"),
            AuthError::Refresh(RefreshError::Reused) => {
                write!(
                    f,
                    "refresh уже предъявляли: все токены этого входа погашены, войди заново"
                )
            }
            AuthError::Refresh(..) => write!(f, "refresh не принят: войди заново"),
            AuthError::OnceInvalid => write!(f, "ссылка недействительна или устарела"),
            AuthError::NoEmail => write!(f, "у учётной записи нет почты"),
            AuthError::Storage(..) => write!(f, "учётная запись не сохранилась, попробуй ещё раз"),
            AuthError::Password(..) => write!(f, "пароль не обработан, попробуй ещё раз"),
            AuthError::Random => write!(f, "генератор случайных чисел недоступен"),
            AuthError::Queue(..) => {
                write!(f, "удаление сейчас не поставить в очередь, повтори позже")
            }
        }
    }
}

impl StdError for AuthError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            AuthError::Storage(error) => Some(error),
            AuthError::Profile(error) => Some(error),
            AuthError::Password(error) => Some(error),
            _ => None,
        }
    }
}
