// Репозиторий отвечает на языке базы: «нарушено ограничение users_email_key», «таймаут». Сервис
// должен отвечать на языке предметной области: «почта занята», «временно недоступно». Если
// переводить всё в «внутреннюю ошибку», клиент не узнает, что ему нужно поменять почту. Цель:
// научить `ServiceError` строиться из `RepoError` так, чтобы `?` в сервисе переводил ошибки сам
// и ничего не терял (имена ограничений смотри в тестах): настоящую причину нельзя выбрасывать даже там, где ошибка уходит в «внутреннюю».

use std::error::Error;
use std::fmt;

#[derive(Debug)]
pub enum RepoError {
    NotFound { id: u64 },
    UniqueViolation { constraint: String },
    Timeout,
    Io(String),
}

impl fmt::Display for RepoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RepoError::NotFound { id } => write!(f, "строка {id} не найдена"),
            RepoError::UniqueViolation { constraint } => write!(f, "нарушено ограничение {constraint}"),
            RepoError::Timeout => write!(f, "таймаут запроса"),
            RepoError::Io(text) => write!(f, "ошибка ввода-вывода: {text}"),
        }
    }
}

impl Error for RepoError {}

#[derive(Debug)]
pub enum ServiceError {
    UserNotFound(u64),
    UsernameTaken,
    EmailTaken,
    AlreadyFollowing,
    Unavailable,
    Internal(Box<dyn Error + Send + Sync>),
}

impl fmt::Display for ServiceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ServiceError::UserNotFound(id) => write!(f, "пользователь {id} не найден"),
            ServiceError::UsernameTaken => write!(f, "имя занято"),
            ServiceError::EmailTaken => write!(f, "почта занята"),
            ServiceError::AlreadyFollowing => write!(f, "вы уже подписаны"),
            ServiceError::Unavailable => write!(f, "временно недоступно"),
            ServiceError::Internal(_) => write!(f, "внутренняя ошибка"),
        }
    }
}

impl Error for ServiceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            ServiceError::Internal(cause) => Some(cause.as_ref()),
            _ => None,
        }
    }
}

impl From<RepoError> for ServiceError {
    fn from(err: RepoError) -> ServiceError {
        ServiceError::Internal(Box::new(err))
    }
}

pub trait UserRepo {
    fn insert(&self, username: &str, email: &str) -> Result<u64, RepoError>;
    fn exists(&self, id: u64) -> Result<(), RepoError>;
    fn add_follow(&self, who: u64, target: u64) -> Result<(), RepoError>;
}

pub fn register(repo: &dyn UserRepo, username: &str, email: &str) -> Result<u64, ServiceError> {
    Ok(repo.insert(username, email)?)
}

pub fn follow(repo: &dyn UserRepo, who: u64, target: u64) -> Result<(), ServiceError> {
    repo.exists(target)?;
    repo.add_follow(who, target)?;
    Ok(())
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    struct Script {
        insert: RefCell<Option<Result<u64, RepoError>>>,
        exists: RefCell<Option<Result<(), RepoError>>>,
        follow: RefCell<Option<Result<(), RepoError>>>,
    }

    impl Script {
        fn new() -> Script {
            Script { insert: RefCell::new(None), exists: RefCell::new(None), follow: RefCell::new(None) }
        }
        fn insert(self, r: Result<u64, RepoError>) -> Script {
            *self.insert.borrow_mut() = Some(r);
            self
        }
        fn exists(self, r: Result<(), RepoError>) -> Script {
            *self.exists.borrow_mut() = Some(r);
            self
        }
        fn follow(self, r: Result<(), RepoError>) -> Script {
            *self.follow.borrow_mut() = Some(r);
            self
        }
    }

    impl UserRepo for Script {
        fn insert(&self, _: &str, _: &str) -> Result<u64, RepoError> {
            self.insert.borrow_mut().take().expect("insert не ожидался")
        }
        fn exists(&self, _: u64) -> Result<(), RepoError> {
            self.exists.borrow_mut().take().expect("exists не ожидался")
        }
        fn add_follow(&self, _: u64, _: u64) -> Result<(), RepoError> {
            self.follow.borrow_mut().take().expect("add_follow не ожидался")
        }
    }

    fn unique(name: &str) -> RepoError {
        RepoError::UniqueViolation { constraint: name.to_string() }
    }

    #[test]
    fn success_passes_through() {
        let repo = Script::new().insert(Ok(7));
        assert_eq!(register(&repo, "alice", "a@x.io").unwrap(), 7, "успех не меняется");
    }

    #[test]
    fn each_constraint_has_its_own_meaning() {
        let err = register(&Script::new().insert(Err(unique("users_username_key"))), "a", "b").unwrap_err();
        assert!(matches!(err, ServiceError::UsernameTaken), "users_username_key значит «имя занято», а было {err:?}");
        let err = register(&Script::new().insert(Err(unique("users_email_key"))), "a", "b").unwrap_err();
        assert!(matches!(err, ServiceError::EmailTaken), "users_email_key значит «почта занята», а было {err:?}");
        let err = follow(&Script::new().exists(Ok(())).follow(Err(unique("follows_pkey"))), 1, 2).unwrap_err();
        assert!(matches!(err, ServiceError::AlreadyFollowing), "повторная подписка, а было {err:?}");
    }

    #[test]
    fn unknown_constraint_is_internal_but_remembers_what_it_was() {
        let err = register(&Script::new().insert(Err(unique("users_pkey"))), "a", "b").unwrap_err();
        assert!(matches!(err, ServiceError::Internal(_)), "незнакомое ограничение это наш баг, а не вина клиента: {err:?}");
        let source = err.source().expect("причина должна остаться");
        let repo_err = source.downcast_ref::<RepoError>().expect("причина это исходный RepoError, а не строка");
        assert!(matches!(repo_err, RepoError::UniqueViolation { constraint } if constraint == "users_pkey"), "и ограничение в ней то же");
    }

    #[test]
    fn not_found_keeps_the_id() {
        let err = follow(&Script::new().exists(Err(RepoError::NotFound { id: 99 })), 1, 99).unwrap_err();
        assert!(matches!(err, ServiceError::UserNotFound(99)), "клиенту нужно знать, какого пользователя нет: {err:?}");
    }

    #[test]
    fn timeout_is_a_temporary_condition() {
        let err = register(&Script::new().insert(Err(RepoError::Timeout)), "a", "b").unwrap_err();
        assert!(matches!(err, ServiceError::Unavailable), "таймаут это «попробуй позже», а не «внутренняя ошибка»: {err:?}");
    }

    #[test]
    fn io_failure_is_internal_with_the_original_cause() {
        let err = register(&Script::new().insert(Err(RepoError::Io("disk".into()))), "a", "b").unwrap_err();
        assert!(matches!(err, ServiceError::Internal(_)), "сбой диска: {err:?}");
        assert_eq!(err.source().map(|s| s.to_string()), Some("ошибка ввода-вывода: disk".to_string()), "причина сохранена");
    }
}
