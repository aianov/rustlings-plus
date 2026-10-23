// Две регистрации одного ника, пришедшие одновременно, не должны проходить обе, и это должен
// гарантировать сам репозиторий, потому что только он видит все данные. Хранилище `Store` даёт
// доступ к данным через `with` (замок на время замыкания). Тест ставит всех участников на
// барьер после каждого вызова `with`: любое решение, у которого между проверкой и записью
// замок отпускался, получит дубликаты.
// Цель: `create_if_absent` и `UserService::register`: ник нормализуется, пустой отклоняется,
// повтор это `Taken`, а отказ не тратит номер пользователя.

use std::collections::HashMap;
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
    fn create_if_absent(&self, name: &str) -> Result<User, RepoError>;
    fn count(&self) -> usize;
}

#[derive(Default)]
pub struct State {
    pub last_id: u64,
    pub by_name: HashMap<String, User>,
}

pub struct Store {
    state: Mutex<State>,
    after_unlock: Box<dyn Fn() + Send + Sync>,
}

impl Store {
    pub fn new() -> Store {
        Store::with_hook(|| {})
    }

    pub fn with_hook(hook: impl Fn() + Send + Sync + 'static) -> Store {
        Store { state: Mutex::new(State::default()), after_unlock: Box::new(hook) }
    }

    pub fn with<R>(&self, f: impl FnOnce(&mut State) -> R) -> R {
        let result = f(&mut self.state.lock().unwrap());
        (self.after_unlock)();
        result
    }
}

impl Default for Store {
    fn default() -> Self {
        Store::new()
    }
}

pub struct InMemoryUsers {
    store: Store,
}

impl InMemoryUsers {
    pub fn new(store: Store) -> InMemoryUsers {
        InMemoryUsers { store }
    }
}

impl UserRepository for InMemoryUsers {
    fn create_if_absent(&self, name: &str) -> Result<User, RepoError> {
        todo!()
    }

    fn count(&self) -> usize {
        self.store.with(|state| state.by_name.len())
    }
}

#[derive(Debug, PartialEq)]
pub enum RegisterError {
    Empty,
    Taken,
}

pub struct UserService<R: UserRepository> {
    repo: R,
}

impl<R: UserRepository> UserService<R> {
    pub fn new(repo: R) -> Self {
        Self { repo }
    }

    pub fn register(&self, raw: &str) -> Result<User, RegisterError> {
        todo!()
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Barrier;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    fn run_parallel<T: Send>(n: usize, job: impl Fn(usize) -> T + Sync) -> Vec<T> {
        std::thread::scope(|scope| {
            let job = &job;
            let handles: Vec<_> = (0..n)
                .map(|i| std::thread::Builder::new().spawn_scoped(scope, move || job(i)).unwrap())
                .collect();
            handles.into_iter().map(|h| h.join().unwrap()).collect()
        })
    }

    fn service() -> UserService<InMemoryUsers> {
        UserService::new(InMemoryUsers::new(Store::new()))
    }

    #[test]
    fn sequential_registration() {
        let service = service();
        assert_eq!(service.register("  Alice ").unwrap().name, "alice", "ник нормализуется в service");
        assert_eq!(service.register("alice"), Err(RegisterError::Taken), "повтор это Taken");
        assert_eq!(service.register("  "), Err(RegisterError::Empty), "пустой ник");
    }

    #[test]
    fn conflict_does_not_burn_an_id() {
        let service = service();
        assert_eq!(service.register("alice").unwrap().id, 1, "первый id");
        assert!(service.register("alice").is_err(), "дубликат отклонён");
        assert_eq!(
            service.register("bob").unwrap().id,
            2,
            "отказ не должен расходовать номер: id выдаётся только при успешной вставке"
        );
    }

    #[test]
    fn exactly_one_of_many_racers_wins() {
        const RACERS: usize = 8;
        let armed = Arc::new(AtomicBool::new(true));
        let gate = Arc::new(Barrier::new(RACERS));
        let store = {
            let (armed, gate) = (Arc::clone(&armed), Arc::clone(&gate));
            Store::with_hook(move || {
                if armed.load(Ordering::SeqCst) {
                    gate.wait();
                }
            })
        };
        let service = UserService::new(InMemoryUsers::new(store));
        let results = run_parallel(RACERS, |_| service.register("alice"));
        armed.store(false, Ordering::SeqCst);
        let wins = results.iter().filter(|r| r.is_ok()).count();
        assert_eq!(
            wins, 1,
            "одновременно регистрировали один ник: успешных должно быть ровно одна, а их {wins}. Между проверкой и записью замок отпускать нельзя"
        );
        assert!(
            results.iter().filter(|r| r.is_err()).all(|r| *r == Err(RegisterError::Taken)),
            "остальные получают Taken"
        );
        assert_eq!(service.repo.count(), 1, "в хранилище один пользователь");
    }

    #[test]
    fn different_names_do_not_conflict() {
        let service = service();
        let results = run_parallel(16, |i| service.register(&format!("user{i}")));
        assert!(results.iter().all(|r| r.is_ok()), "разные ники не мешают друг другу");
        assert_eq!(service.repo.count(), 16, "все шестнадцать сохранены");
    }
}
