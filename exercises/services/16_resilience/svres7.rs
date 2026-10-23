// Клиент отправил «создать пост», сеть мигнула, ответа нет. Повторить нельзя (вдруг пост уже создан и получится два), не повторять
// тоже нельзя (вдруг не создан). Выход: клиент присылает вместе с запросом ключ `Idempotency-Key`, и сервис отвечает на повтор с
// тем же ключом тем же результатом, не создавая второго поста.
// Цель: `Posts::create` с ключом идемпотентности. Ключи принадлежат пользователю и живут `ttl`. Один ключ с другим телом запроса это
// ошибка клиента. Запрос без ключа работает как раньше. Неудавшийся запрос ключ не занимает.


use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;
use tokio::time::Instant;

fn main() {}

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    InvalidArgument,
    KeyReuse,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Created {
    pub id: u64,
    /// Ответ повторён по ключу, нового поста не появилось.
    pub replayed: bool,
}

struct Entry {
    text: String,
    id: u64,
    at: Instant,
}

#[derive(Default)]
struct Inner {
    next_id: u64,
    posts: Vec<(u64, String)>,
    keys: HashMap<(u64, String), Entry>,
}

pub struct Posts {
    ttl: Duration,
    inner: Mutex<Inner>,
}

impl Posts {
    pub fn new(ttl: Duration) -> Posts {
        Posts { ttl, inner: Mutex::new(Inner::default()) }
    }

    pub fn count(&self) -> usize {
        self.inner.lock().unwrap().posts.len()
    }

    pub async fn create(&self, user: u64, key: Option<&str>, text: &str) -> Result<Created, Error> {
        if text.trim().is_empty() {
            return Err(Error::InvalidArgument);
        }
        let mut inner = self.inner.lock().unwrap();
        inner.next_id += 1;
        let id = inner.next_id;
        inner.posts.push((user, text.to_string()));
        let _ = (key, self.ttl);
        Ok(Created { id, replayed: false })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: Duration = Duration::from_secs(24 * 3600);

    #[tokio::test(start_paused = true)]
    async fn the_same_key_returns_the_same_post() {
        let posts = Posts::new(DAY);
        let first = posts.create(1, Some("k1"), "привет").await.unwrap();
        let second = posts.create(1, Some("k1"), "привет").await.unwrap();
        assert!(!first.replayed);
        assert!(second.replayed, "повтор с тем же ключом должен быть помечен как повтор");
        assert_eq!(first.id, second.id, "повтор должен вернуть тот же id");
        assert_eq!(posts.count(), 1, "второго поста быть не должно");
    }

    #[tokio::test(start_paused = true)]
    async fn without_a_key_every_request_creates() {
        let posts = Posts::new(DAY);
        let a = posts.create(1, None, "раз").await.unwrap();
        let b = posts.create(1, None, "раз").await.unwrap();
        assert_ne!(a.id, b.id);
        assert_eq!(posts.count(), 2, "запросы без ключа не дедуплицируются");
    }

    #[tokio::test(start_paused = true)]
    async fn the_same_key_with_another_body_is_a_client_error() {
        let posts = Posts::new(DAY);
        posts.create(1, Some("k"), "первый").await.unwrap();
        assert_eq!(posts.create(1, Some("k"), "другой").await, Err(Error::KeyReuse), "ключ уже связан с другим запросом");
        assert_eq!(posts.count(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn keys_belong_to_a_user() {
        let posts = Posts::new(DAY);
        let a = posts.create(1, Some("same"), "текст").await.unwrap();
        let b = posts.create(2, Some("same"), "текст").await.unwrap();
        assert_ne!(a.id, b.id, "один и тот же ключ у разных пользователей это разные запросы");
        assert!(!b.replayed);
        assert_eq!(posts.count(), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn a_failed_request_does_not_claim_the_key() {
        let posts = Posts::new(DAY);
        assert_eq!(posts.create(1, Some("k"), "   ").await, Err(Error::InvalidArgument));
        let ok = posts.create(1, Some("k"), "исправил").await.unwrap();
        assert!(!ok.replayed, "неудачная попытка не должна занимать ключ");
        assert_eq!(posts.count(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn keys_expire() {
        let posts = Posts::new(DAY);
        let first = posts.create(1, Some("k"), "текст").await.unwrap();
        tokio::time::advance(DAY - Duration::from_secs(60)).await;
        assert!(posts.create(1, Some("k"), "текст").await.unwrap().replayed, "за минуту до конца срока ключ ещё действует");
        tokio::time::advance(Duration::from_secs(120)).await;
        let later = posts.create(1, Some("k"), "текст").await.unwrap();
        assert!(!later.replayed, "срок ключа вышел, это новый запрос");
        assert_ne!(later.id, first.id);
        assert_eq!(posts.count(), 2);
    }
}
