use futures::future::BoxFuture;
use std::sync::Mutex;

#[derive(Clone, Debug, PartialEq)]
pub struct User {
    pub id: u64,
    pub name: String,
}

#[derive(Debug, PartialEq)]
pub enum RepoError {
    Conflict,
}

pub trait UserRepository: Send + Sync {
    fn find_by_name<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Option<User>>;
    fn insert<'a>(&'a self, name: &'a str) -> BoxFuture<'a, User>;
    fn create_if_absent<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<User, RepoError>>;
    fn count(&self) -> BoxFuture<'_, usize>;
}

pub struct Store {
    users: Mutex<Vec<User>>,
    after_unlock: Box<dyn Fn() + Send + Sync>,
}

impl Store {
    pub fn new() -> Store {
        Store::with_hook(|| {})
    }

    pub fn with_hook(hook: impl Fn() + Send + Sync + 'static) -> Store {
        Store { users: Mutex::new(Vec::new()), after_unlock: Box::new(hook) }
    }

    pub fn with<R>(&self, f: impl FnOnce(&mut Vec<User>) -> R) -> R {
        let result = f(&mut self.users.lock().unwrap());
        (self.after_unlock)();
        result
    }
}

impl Default for Store {
    fn default() -> Self {
        Store::new()
    }
}

#[derive(Default)]
pub struct InMemoryUsers {
    store: Store,
}

impl InMemoryUsers {
    pub fn new(store: Store) -> InMemoryUsers {
        InMemoryUsers { store }
    }
}

impl UserRepository for InMemoryUsers {
    fn find_by_name<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Option<User>> {
        Box::pin(async move { self.store.with(|users| users.iter().find(|u| u.name == name).cloned()) })
    }

    fn insert<'a>(&'a self, name: &'a str) -> BoxFuture<'a, User> {
        Box::pin(async move {
            self.store.with(|users| {
                let user = User { id: users.len() as u64 + 1, name: name.to_string() };
                users.push(user.clone());
                user
            })
        })
    }

    fn create_if_absent<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<User, RepoError>> {
        Box::pin(async move {
            self.store.with(|users| {
                if users.iter().any(|u| u.name == name) {
                    return Err(RepoError::Conflict);
                }
                let user = User { id: users.len() as u64 + 1, name: name.to_string() };
                users.push(user.clone());
                Ok(user)
            })
        })
    }

    fn count(&self) -> BoxFuture<'_, usize> {
        Box::pin(async move { self.store.with(|users| users.len()) })
    }
}

#[derive(Debug, PartialEq)]
pub enum RegisterError {
    Taken,
}

pub struct UserService<R: UserRepository> {
    repo: R,
}

impl<R: UserRepository> UserService<R> {
    pub fn new(repo: R) -> Self {
        Self { repo }
    }

    pub async fn register(&self, raw: &str) -> Result<User, RegisterError> {
        let name = raw.trim().to_lowercase();
        self.repo.create_if_absent(&name).await.map_err(|RepoError::Conflict| RegisterError::Taken)
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::future::join_all;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;
    use tokio::sync::Barrier;

    struct BarrierAfterFind {
        inner: InMemoryUsers,
        barrier: Barrier,
    }

    impl UserRepository for BarrierAfterFind {
        fn find_by_name<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Option<User>> {
            Box::pin(async move {
                let found = self.inner.find_by_name(name).await;
                self.barrier.wait().await;
                found
            })
        }

        fn insert<'a>(&'a self, name: &'a str) -> BoxFuture<'a, User> {
            self.inner.insert(name)
        }

        fn create_if_absent<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<User, RepoError>> {
            self.inner.create_if_absent(name)
        }

        fn count(&self) -> BoxFuture<'_, usize> {
            self.inner.count()
        }
    }

    #[tokio::test]
    async fn sequential_duplicate_is_rejected() {
        let service = UserService::new(InMemoryUsers::default());
        assert!(service.register("alice").await.is_ok(), "первая регистрация");
        assert_eq!(service.register("Alice").await, Err(RegisterError::Taken), "повтор");
    }

    #[tokio::test]
    async fn concurrent_duplicates_do_not_both_win() {
        const RACERS: usize = 8;
        let repo = BarrierAfterFind { inner: InMemoryUsers::default(), barrier: Barrier::new(RACERS) };
        let service = Arc::new(UserService::new(repo));
        let all = join_all((0..RACERS).map(|_| {
            let service = Arc::clone(&service);
            async move { service.register("alice").await }
        }));
        let results = tokio::time::timeout(Duration::from_secs(5), all)
            .await
            .expect("регистрации повисли на барьере: после исправления service не обязан звать find_by_name");
        let wins = results.iter().filter(|r| r.is_ok()).count();
        assert_eq!(
            wins, 1,
            "одновременные регистрации одного ника: успешной должна быть ровно одна, а их {wins}. Проверка и вставка должны быть одной операцией репозитория"
        );
        assert_eq!(service.repo.count().await, 1, "в хранилище ровно один alice");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 6)]
    async fn repository_operation_is_atomic_on_its_own() {
        const RACERS: usize = 4;
        let armed = Arc::new(AtomicBool::new(true));
        let gate = Arc::new(std::sync::Barrier::new(RACERS));
        let store = {
            let (armed, gate) = (Arc::clone(&armed), Arc::clone(&gate));
            Store::with_hook(move || {
                if armed.load(Ordering::SeqCst) {
                    tokio::task::block_in_place(|| gate.wait());
                }
            })
        };
        let repo = Arc::new(InMemoryUsers::new(store));
        let tasks: Vec<_> = (0..RACERS)
            .map(|_| {
                let repo = Arc::clone(&repo);
                tokio::spawn(async move { repo.create_if_absent("bob").await })
            })
            .collect();
        let results = tokio::time::timeout(Duration::from_secs(5), join_all(tasks))
            .await
            .expect("гонка повисла: create_if_absent должен обращаться к Store::with не более раза");
        armed.store(false, Ordering::SeqCst);
        let wins = results.into_iter().map(|r| r.unwrap()).filter(|r| r.is_ok()).count();
        assert_eq!(wins, 1, "create_if_absent пропустил {wins} одинаковых ников: проверка и запись должны идти под одним замком");
        assert_eq!(repo.count().await, 1, "в хранилище один пользователь");
    }
}
