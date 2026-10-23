// Для значка «онлайн» нужно хранилище «ключ и значение» с памятью об истории: кто в каком статусе
// сейчас, что менялось, и чтобы забытые статусы пропадали сами. В JetStream для этого есть KV
// (бакет это stream с надстройкой). Цель: open_presence заводит бакет, где каждый статус живёт
// не дольше `ttl` с последней записи; set_status возвращает номер ревизии записи; status_of отдаёт
// статус или None; forget удаляет; changes подписывается на бакет и собирает всё, что в нём есть
// и что в нём происходит, пока между событиями не пройдёт больше `quiet`. Удаление это изменение
// со значением None.


use async_nats::jetstream::Context;
use async_nats::jetstream::kv::{Config, Operation, Store};
use futures::StreamExt;
use std::time::Duration;

fn main() {}

#[derive(Debug, PartialEq)]
pub struct Change {
    pub key: String,
    pub value: Option<String>,
    pub revision: u64,
}

pub async fn open_presence(js: &Context, bucket: &str, ttl: Duration) -> Result<Store, String> {
    let _ = (js, bucket, ttl, Config::default());
    todo!()
}

pub async fn set_status(kv: &Store, user: &str, status: &str) -> Result<u64, String> {
    let _ = (kv, user, status);
    todo!()
}

pub async fn status_of(kv: &Store, user: &str) -> Result<Option<String>, String> {
    let _ = (kv, user);
    todo!()
}

pub async fn forget(kv: &Store, user: &str) -> Result<(), String> {
    let _ = (kv, user);
    todo!()
}

pub async fn changes(kv: &Store, quiet: Duration) -> Result<Vec<Change>, String> {
    let _ = (kv, quiet);
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::nats::{Scope, connect, js};

    async fn bucket(scope: &Scope, ttl: Duration) -> Store {
        let js = js(&connect().await);
        let name = scope.name("presence");
        scope.track_bucket(&name);
        open_presence(&js, &name, ttl).await.expect("бакет")
    }

    #[tokio::test]
    async fn statuses_are_stored_and_read_back() {
        let scope = Scope::new();
        let kv = bucket(&scope, Duration::from_secs(60)).await;
        let r1 = set_status(&kv, "alice", "online").await.unwrap();
        let r2 = set_status(&kv, "bob", "away").await.unwrap();
        let r3 = set_status(&kv, "alice", "busy").await.unwrap();
        assert!(r1 < r2 && r2 < r3, "ревизии растут с каждой записью: {r1} {r2} {r3}");
        assert_eq!(status_of(&kv, "alice").await.unwrap().as_deref(), Some("busy"), "последняя запись побеждает");
        assert_eq!(status_of(&kv, "bob").await.unwrap().as_deref(), Some("away"), "bob");
        assert_eq!(status_of(&kv, "carol").await.unwrap(), None, "неизвестный ключ это None, а не ошибка");
    }

    #[tokio::test]
    async fn forgetting_removes_the_status() {
        let scope = Scope::new();
        let kv = bucket(&scope, Duration::from_secs(60)).await;
        set_status(&kv, "alice", "online").await.unwrap();
        forget(&kv, "alice").await.unwrap();
        assert_eq!(status_of(&kv, "alice").await.unwrap(), None, "после удаления значения нет");
        forget(&kv, "nobody").await.expect("удаление несуществующего ключа не ошибка");
    }

    #[tokio::test]
    async fn the_watch_shows_current_values_and_then_new_changes() {
        let scope = Scope::new();
        let kv = bucket(&scope, Duration::from_secs(60)).await;
        set_status(&kv, "alice", "online").await.unwrap();
        set_status(&kv, "bob", "away").await.unwrap();
        set_status(&kv, "bob", "online").await.unwrap();
        let writer = kv.clone();
        let later = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(150)).await;
            set_status(&writer, "carol", "busy").await.unwrap();
            forget(&writer, "alice").await.unwrap();
        });
        let seen = changes(&kv, Duration::from_millis(700)).await.unwrap();
        later.await.unwrap();
        let summary: Vec<(String, Option<String>)> = seen.iter().map(|c| (c.key.clone(), c.value.clone())).collect();
        assert_eq!(
            summary,
            vec![
                ("alice".to_string(), Some("online".to_string())),
                ("bob".to_string(), Some("online".to_string())),
                ("carol".to_string(), Some("busy".to_string())),
                ("alice".to_string(), None),
            ],
            "сначала актуальные значения (у bob только последнее, история в бакете одна запись), потом новые изменения; удаление приходит со значением None"
        );
        assert!(seen.windows(2).all(|w| w[0].revision < w[1].revision), "ревизии возрастают");
    }

    #[tokio::test]
    async fn a_status_expires_by_itself() {
        let scope = Scope::new();
        let kv = bucket(&scope, Duration::from_secs(1)).await;
        set_status(&kv, "alice", "online").await.unwrap();
        assert!(status_of(&kv, "alice").await.unwrap().is_some(), "сразу после записи статус виден");
        tokio::time::sleep(Duration::from_millis(2600)).await;
        assert_eq!(status_of(&kv, "alice").await.unwrap(), None, "через ttl статус пропал сам: так «онлайн» не залипает навсегда");
    }
}
