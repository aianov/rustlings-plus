use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Debug, Clone, PartialEq)]
struct User {
    id: u32,
    name: String,
}

trait UserRepo {
    async fn find(&self, id: u32) -> Option<User>;
    async fn save(&self, user: User);
    async fn count(&self) -> usize;
}

struct MemRepo {
    users: Mutex<HashMap<u32, User>>,
}

impl MemRepo {
    fn new() -> MemRepo {
        MemRepo { users: Mutex::new(HashMap::new()) }
    }
}

impl UserRepo for MemRepo {
    async fn find(&self, id: u32) -> Option<User> {
        self.users.lock().unwrap().get(&id).cloned()
    }

    async fn save(&self, user: User) {
        self.users.lock().unwrap().insert(user.id, user);
    }

    async fn count(&self) -> usize {
        self.users.lock().unwrap().len()
    }
}

async fn rename<R: UserRepo>(repo: &R, id: u32, name: &str) -> bool {
    match repo.find(id).await {
        Some(mut user) => {
            user.name = name.to_string();
            repo.save(user).await;
            true
        }
        None => false,
    }
}

#[tokio::main]
async fn main() {
    let repo = MemRepo::new();
    repo.save(User { id: 1, name: "Аня".into() }).await;
    println!("{}", rename(&repo, 1, "Аня Иванова").await);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user(id: u32, name: &str) -> User {
        User { id, name: name.to_string() }
    }

    #[tokio::test]
    async fn saved_user_is_found() {
        let repo = MemRepo::new();
        repo.save(user(1, "Аня")).await;
        assert_eq!(repo.find(1).await, Some(user(1, "Аня")), "сохранённого пользователя find должен вернуть целиком");
        assert_eq!(repo.find(2).await, None, "чужого id в хранилище нет");
    }

    #[tokio::test]
    async fn save_overwrites_by_id_and_count_follows() {
        let repo = MemRepo::new();
        assert_eq!(repo.count().await, 0, "новое хранилище пусто");
        repo.save(user(1, "Аня")).await;
        repo.save(user(2, "Боря")).await;
        repo.save(user(1, "Анна")).await;
        assert_eq!(repo.count().await, 2, "повторное сохранение того же id заменяет запись, а не добавляет новую");
        assert_eq!(repo.find(1).await.unwrap().name, "Анна", "должна остаться последняя версия");
    }

    #[tokio::test]
    async fn rename_works_through_the_trait_only() {
        let repo = MemRepo::new();
        repo.save(user(7, "Витя")).await;
        assert!(rename(&repo, 7, "Виктор").await, "пользователь есть: rename должен вернуть true");
        assert_eq!(repo.find(7).await, Some(user(7, "Виктор")), "имя должно измениться, id остаться");
        assert!(!rename(&repo, 8, "Никто").await, "пользователя нет: rename возвращает false");
        assert_eq!(repo.count().await, 1, "rename несуществующего не должен создавать пользователя");
    }
}
