use std::error::Error;
use std::fmt;

use contracts::{ErrorBody, UserId, UserView};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Session {
    Anonymous,
    Introduced(UserView),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionError {
    NotIntroduced,
    AlreadyIntroduced(String),
}

impl Session {
    pub fn hello(&self, user: UserView) -> Result<Session, SessionError> {
        match self {
            Session::Anonymous => Ok(Session::Introduced(user)),
            Session::Introduced(me) => Err(SessionError::AlreadyIntroduced(me.username.clone())),
        }
    }

    pub fn bye(&self) -> Result<(Session, UserView), SessionError> {
        match self {
            Session::Anonymous => Err(SessionError::NotIntroduced),
            Session::Introduced(me) => Ok((Session::Anonymous, me.clone())),
        }
    }

    pub fn actor(&self) -> Result<UserId, SessionError> {
        match self {
            Session::Anonymous => Err(SessionError::NotIntroduced),
            Session::Introduced(me) => Ok(me.id),
        }
    }
}

impl SessionError {
    pub fn code(&self) -> &'static str {
        match self {
            SessionError::NotIntroduced => "unauthorized",
            SessionError::AlreadyIntroduced(..) => "conflict",
        }
    }

    pub fn to_body(&self) -> ErrorBody {
        ErrorBody {
            code: self.code().to_string(),
            message: self.to_string(),
        }
    }
}

impl fmt::Display for SessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SessionError::NotIntroduced => {
                write!(f, "сначала представься: session.hello {{username}}")
            }
            SessionError::AlreadyIntroduced(name) => {
                write!(
                    f,
                    "соединение уже представлено как @{name}, сначала session.bye"
                )
            }
        }
    }
}

impl Error for SessionError {}
