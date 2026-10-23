// Репозиторий это трейт: сервис знает контракт «найти и сохранить», а не то, что под ним лежит.
// Контракт готов, и методы берут `&self`: репозиторий общий для всех запросов, изменяемость
// спрятана внутри. Осталось написать реализацию для тестов и первого запуска.
// TODO: Реализуй `UserRepository` для `InMemoryUsers`: id выдаются с единицы по порядку,
// `find_by_name` ищет точное совпадение, `count` считает всех. Всё под одним `Mutex`.

use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Clone, Debug, PartialEq)]
pub struct User {
    pub id: u64,
    pub name: String,
}

pub trait UserRepository: Send + Sync {
    fn insert(&self, name: &str) -> User;
    fn find(&self, id: u64) -> Option<User>;
    fn find_by_name(&self, name: &str) -> Option<User>;
    fn count(&self) -> usize;
}

#[derive(Default)]
struct State {
    last_id: u64,
    users: HashMap<u64, User>,
}

#[derive(Default)]
pub struct InMemoryUsers {
    state: Mutex<State>,
}

impl UserRepository for InMemoryUsers {
    fn insert(&self, name: &str) -> User {
        todo!()
    }

    fn find(&self, id: u64) -> Option<User> {
        todo!()
    }

    fn find_by_name(&self, name: &str) -> Option<User> {
        todo!()
    }

    fn count(&self) -> usize {
        todo!()
    }
}

fn main() {
    let repo = InMemoryUsers::default();
    println!("{:?}", repo.insert("alice"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn run_parallel<T: Send>(n: usize, job: impl Fn(usize) -> T + Sync) -> Vec<T> {
        std::thread::scope(|scope| {
            let job = &job;
            let handles: Vec<_> = (0..n)
                .map(|i| std::thread::Builder::new().spawn_scoped(scope, move || job(i)).unwrap())
                .collect();
            handles.into_iter().map(|h| h.join().unwrap()).collect()
        })
    }

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn ids_start_at_one_and_grow() {
        let repo = InMemoryUsers::default();
        assert_eq!(repo.insert("alice").id, 1, "первый пользователь получает id 1");
        assert_eq!(repo.insert("bob").id, 2, "второй получает id 2");
        assert_eq!(repo.count(), 2, "count должен считать всех сохранённых");
    }

    #[test]
    fn finds_by_id_and_by_name() {
        let repo = InMemoryUsers::default();
        let alice = repo.insert("alice");
        assert_eq!(repo.find(alice.id), Some(alice.clone()), "find по id");
        assert_eq!(repo.find_by_name("alice"), Some(alice), "find_by_name по точному совпадению");
        assert_eq!(repo.find(99), None, "неизвестный id даёт None");
        assert_eq!(repo.find_by_name("Alice"), None, "репозиторий не нормализует: это дело service");
    }

    #[test]
    fn works_behind_a_trait_object() {
        assert_send_sync::<InMemoryUsers>();
        let repo: Arc<dyn UserRepository> = Arc::new(InMemoryUsers::default());
        repo.insert("carol");
        assert_eq!(repo.count(), 1, "через &dyn UserRepository вызовы работают так же");
    }

    #[test]
    fn many_threads_get_unique_ids() {
        let repo = InMemoryUsers::default();
        let mut ids: Vec<u64> = run_parallel(8, |t| {
            (0..100).map(|i| repo.insert(&format!("u{t}_{i}")).id).collect::<Vec<_>>()
        })
        .into_iter()
        .flatten()
        .collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), 800, "id должны быть уникальны: выдача id и вставка идут под одним замком");
        assert_eq!(repo.count(), 800, "ни одна вставка не должна потеряться");
    }
}
