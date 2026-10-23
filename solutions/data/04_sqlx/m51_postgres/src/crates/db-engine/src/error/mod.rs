use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Violation {
    Unique,
    ForeignKey,
    Check,
    NotNull,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DbError {
    Constraint {
        violation: Violation,
        constraint: String,
    },
    BadSchema(String),
    Unavailable(String),
    Query(String),
}

impl DbError {
    pub fn is_unique(&self, constraint: &str) -> bool {
        matches!(self, DbError::Constraint { violation: Violation::Unique, constraint: own } if own == constraint)
    }
}

impl fmt::Display for DbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DbError::Constraint {
                violation,
                constraint,
            } => write!(f, "нарушено ограничение {constraint} ({violation:?})"),
            DbError::BadSchema(name) => write!(f, "«{name}» не годится в имя схемы"),
            DbError::Unavailable(why) => write!(f, "Postgres недоступен: {why}"),
            DbError::Query(why) => write!(f, "запрос к Postgres не прошёл: {why}"),
        }
    }
}

impl std::error::Error for DbError {}

impl From<sqlx::Error> for DbError {
    fn from(error: sqlx::Error) -> DbError {
        match &error {
            sqlx::Error::Database(db) => {
                let violation = match db.code().as_deref() {
                    Some("23505") => Some(Violation::Unique),
                    Some("23503") => Some(Violation::ForeignKey),
                    Some("23514") => Some(Violation::Check),
                    Some("23502") => Some(Violation::NotNull),
                    _ => None,
                };
                match violation {
                    Some(violation) => DbError::Constraint {
                        violation,
                        constraint: db.constraint().unwrap_or_default().to_string(),
                    },
                    None => DbError::Query(db.message().to_string()),
                }
            }
            sqlx::Error::Io(..)
            | sqlx::Error::PoolTimedOut
            | sqlx::Error::PoolClosed
            | sqlx::Error::Tls(..) => DbError::Unavailable(error.to_string()),
            _ => DbError::Query(error.to_string()),
        }
    }
}
