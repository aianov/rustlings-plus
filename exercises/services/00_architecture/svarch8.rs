// Всё работает, пока тест один. Но тест ниже собирает две независимые копии приложения, и
// вторая считает ник занятым, хотя регистрировали его в первой. В проде копия одна, а в тестах их
// десятки, и они идут параллельно в одном процессе.
// Цель: каждая сборка через `bootstrap::build` получает собственные данные. Трейт и сервис не меняй.

pub mod repository {
    use std::collections::HashSet;
    use std::sync::{LazyLock, Mutex};

    pub trait UserRepository: Send + Sync {
        fn exists(&self, name: &str) -> bool;
        fn insert(&self, name: &str);
        fn count(&self) -> usize;
    }

    static USERS: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(Default::default);

    pub struct InMemoryUsers;

    impl UserRepository for InMemoryUsers {
        fn exists(&self, name: &str) -> bool {
            USERS.lock().unwrap().contains(name)
        }

        fn insert(&self, name: &str) {
            USERS.lock().unwrap().insert(name.to_string());
        }

        fn count(&self) -> usize {
            USERS.lock().unwrap().len()
        }
    }
}

pub mod service {
    use super::repository::UserRepository;
    use std::sync::Arc;

    #[derive(Debug, PartialEq)]
    pub enum RegisterError {
        Taken,
    }

    pub struct UserService {
        repo: Arc<dyn UserRepository>,
    }

    impl UserService {
        pub fn new(repo: Arc<dyn UserRepository>) -> Self {
            Self { repo }
        }

        pub fn register(&self, name: &str) -> Result<(), RegisterError> {
            let name = name.trim().to_lowercase();
            if self.repo.exists(&name) {
                return Err(RegisterError::Taken);
            }
            self.repo.insert(&name);
            Ok(())
        }

        pub fn user_count(&self) -> usize {
            self.repo.count()
        }
    }
}

pub mod bootstrap {
    use super::repository::{InMemoryUsers, UserRepository};
    use super::service::UserService;
    use std::sync::Arc;

    pub fn build() -> UserService {
        build_with(Arc::new(InMemoryUsers))
    }

    pub fn build_with(repo: Arc<dyn UserRepository>) -> UserService {
        UserService::new(repo)
    }
}

fn main() {
    let app = bootstrap::build();
    println!("{:?}", app.register("alice"));
}

#[cfg(test)]
mod tests {
    use super::bootstrap::{build, build_with};
    use super::repository::UserRepository;
    use super::service::RegisterError;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn registering_works() {
        let app = build();
        assert_eq!(app.register("Alice"), Ok(()), "первая регистрация проходит");
        assert_eq!(app.register("alice"), Err(RegisterError::Taken), "ник занят независимо от регистра");
    }

    #[test]
    fn two_apps_do_not_share_users() {
        let first = build();
        let second = build();
        first.register("alice").unwrap();
        assert_eq!(
            second.register("alice"),
            Ok(()),
            "у второй копии своё хранилище: состояние не должно быть глобальным"
        );
        assert_eq!(first.user_count(), 1, "в первой копии один пользователь");
        assert_eq!(second.user_count(), 1, "во второй тоже один, чужие не видны");
    }

    #[test]
    fn a_fresh_app_is_empty() {
        let app = build();
        assert_eq!(app.user_count(), 0, "новая сборка начинает с пустого хранилища");
    }

    struct Spy {
        inserts: AtomicUsize,
    }

    impl UserRepository for Spy {
        fn exists(&self, _name: &str) -> bool {
            false
        }

        fn insert(&self, _name: &str) {
            self.inserts.fetch_add(1, Ordering::SeqCst);
        }

        fn count(&self) -> usize {
            self.inserts.load(Ordering::SeqCst)
        }
    }

    #[test]
    fn a_custom_repository_can_be_injected() {
        let spy = Arc::new(Spy { inserts: AtomicUsize::new(0) });
        let app = build_with(spy.clone());
        app.register("bob").unwrap();
        assert_eq!(spy.inserts.load(Ordering::SeqCst), 1, "сервис работает с тем репозиторием, что ему дали");
    }
}
