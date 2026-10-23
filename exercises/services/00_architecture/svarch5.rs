// Сервис держит `Arc<dyn UserRepository>`, чтобы реализацию выбирал bootstrap. Но репозиторий
// асинхронный, и в таком виде код не собирается. Когда он соберётся, сервис ещё должен уметь
// работать внутри `tokio::spawn`, а это накладывает требования на каждую реализацию репозитория.
// Цель: чтобы `Arc<dyn UserRepository>` с обеими реализациями работал, включая запуск из `spawn`.

use futures::future::BoxFuture;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Clone, Debug, PartialEq)]
pub struct User {
    pub id: u64,
    pub name: String,
}

pub trait UserRepository: Send + Sync {
    async fn find(&self, id: u64) -> Option<User>;
}

pub struct InMemoryUsers {
    users: Mutex<HashMap<u64, User>>,
}

impl InMemoryUsers {
    pub fn with(users: Vec<User>) -> Self {
        Self { users: Mutex::new(users.into_iter().map(|u| (u.id, u)).collect()) }
    }
}

impl UserRepository for InMemoryUsers {
    async fn find(&self, id: u64) -> Option<User> {
        self.users.lock().unwrap().get(&id).cloned()
    }
}

pub struct SlowUsers {
    inner: InMemoryUsers,
    delay: Duration,
}

impl SlowUsers {
    pub fn new(inner: InMemoryUsers, delay: Duration) -> Self {
        Self { inner, delay }
    }
}

impl UserRepository for SlowUsers {
    async fn find(&self, id: u64) -> Option<User> {
        let users = self.inner.users.lock().unwrap();
        tokio::time::sleep(self.delay).await;
        users.get(&id).cloned()
    }
}

pub struct UserService {
    repo: Arc<dyn UserRepository>,
}

impl UserService {
    pub fn new(repo: Arc<dyn UserRepository>) -> Self {
        Self { repo }
    }

    pub async fn display_name(&self, id: u64) -> Option<String> {
        self.repo.find(id).await.map(|u| format!("@{}", u.name))
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn alice() -> Vec<User> {
        vec![User { id: 1, name: "alice".to_string() }]
    }

    #[tokio::test]
    async fn service_works_with_in_memory_repo() {
        let service = UserService::new(Arc::new(InMemoryUsers::with(alice())));
        assert_eq!(service.display_name(1).await, Some("@alice".to_string()), "имя с собачкой");
        assert_eq!(service.display_name(2).await, None, "неизвестный id даёт None");
    }

    #[tokio::test]
    async fn service_can_run_inside_spawn() {
        let slow = SlowUsers::new(InMemoryUsers::with(alice()), Duration::from_millis(5));
        let service = Arc::new(UserService::new(Arc::new(slow)));
        let task = {
            let service = Arc::clone(&service);
            tokio::spawn(async move { service.display_name(1).await })
        };
        let name = tokio::time::timeout(Duration::from_secs(5), task)
            .await
            .expect("задача не успела за 5 секунд")
            .unwrap();
        assert_eq!(name, Some("@alice".to_string()), "future сервиса должен быть Send, иначе spawn не соберётся");
    }

    #[tokio::test]
    async fn different_repos_live_in_one_collection() {
        let repos: Vec<Arc<dyn UserRepository>> = vec![
            Arc::new(InMemoryUsers::with(alice())),
            Arc::new(SlowUsers::new(InMemoryUsers::with(alice()), Duration::from_millis(1))),
        ];
        for repo in repos {
            assert!(repo.find(1).await.is_some(), "любая реализация отвечает через один и тот же тип dyn");
        }
    }
}
