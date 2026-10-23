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
        let mut state = self.state.lock().unwrap();
        state.last_id += 1;
        let user = User { id: state.last_id, name: name.to_string() };
        state.users.insert(user.id, user.clone());
        user
    }

    fn find(&self, id: u64) -> Option<User> {
        self.state.lock().unwrap().users.get(&id).cloned()
    }

    fn find_by_name(&self, name: &str) -> Option<User> {
        self.state.lock().unwrap().users.values().find(|u| u.name == name).cloned()
    }

    fn count(&self) -> usize {
        self.state.lock().unwrap().users.len()
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
