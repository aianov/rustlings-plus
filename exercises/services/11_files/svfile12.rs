// Смена аватара: сервис сначала кладёт файл в хранилище, потом пишет ссылку на него в базу. Когда база отказала (или
// процесс умер между двумя шагами), файл остался в хранилище, а в базе о нём ничего нет: сирота лежит вечно, и никто
// не знает, что его пора убрать. Файл и база это два хранилища, одной транзакции на них нет. Цель: загрузка идёт в две
// фазы. Перед записью файла в базе заводится запись «ожидает» с ключом и временем, после успешной записи ссылки
// запись снимается, а старый аватар удаляется. Фоновая `sweep` находит записи «ожидает» старше `GRACE` секунд,
// удаляет их файлы и записи. Свежие загрузки, которые ещё идут, она не трогает.

use bytes::Bytes;
use futures::StreamExt;
use futures::stream::BoxStream;
use std::collections::HashMap;
use std::io;
use std::sync::Mutex;

fn main() {}

pub const GRACE: u64 = 600;

pub type Body = BoxStream<'static, io::Result<Bytes>>;

#[derive(Debug, PartialEq)]
pub enum UploadError {
    Storage,
    Db,
}

#[derive(Default)]
pub struct Store {
    files: Mutex<HashMap<String, Vec<u8>>>,
}

impl Store {
    pub async fn put(&self, key: &str, mut body: Body) -> io::Result<()> {
        let mut data = Vec::new();
        while let Some(chunk) = body.next().await {
            data.extend_from_slice(&chunk?);
        }
        self.files.lock().unwrap().insert(key.to_string(), data);
        Ok(())
    }

    pub async fn delete(&self, key: &str) {
        self.files.lock().unwrap().remove(key);
    }

    pub fn keys(&self) -> Vec<String> {
        let mut keys: Vec<String> = self.files.lock().unwrap().keys().cloned().collect();
        keys.sort();
        keys
    }
}

#[derive(Default)]
struct Rows {
    avatars: HashMap<u64, String>,
    pending: HashMap<String, u64>,
}

#[derive(Default)]
pub struct Db {
    rows: Mutex<Rows>,
    pub fail_commit: std::sync::atomic::AtomicBool,
    pub hold_commit: tokio::sync::Notify,
    pub hold: std::sync::atomic::AtomicBool,
}

impl Db {
    pub async fn begin_upload(&self, key: &str, now: u64) {
        self.rows.lock().unwrap().pending.insert(key.to_string(), now);
    }

    pub async fn commit_avatar(&self, user: u64, key: &str) -> Result<Option<String>, UploadError> {
        if self.hold.load(std::sync::atomic::Ordering::SeqCst) {
            self.hold_commit.notified().await;
        }
        if self.fail_commit.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(UploadError::Db);
        }
        let mut rows = self.rows.lock().unwrap();
        rows.pending.remove(key);
        Ok(rows.avatars.insert(user, key.to_string()))
    }

    pub async fn stale_pending(&self, cutoff: u64) -> Vec<String> {
        let mut keys: Vec<String> = self.rows.lock().unwrap().pending.iter().filter(|(_, started)| **started <= cutoff).map(|(k, _)| k.clone()).collect();
        keys.sort();
        keys
    }

    pub async fn forget_pending(&self, key: &str) {
        self.rows.lock().unwrap().pending.remove(key);
    }

    pub fn avatar_of(&self, user: u64) -> Option<String> {
        self.rows.lock().unwrap().avatars.get(&user).cloned()
    }

    pub fn pending_count(&self) -> usize {
        self.rows.lock().unwrap().pending.len()
    }
}

pub struct Avatars {
    pub store: Store,
    pub db: Db,
}

impl Avatars {
    pub fn new() -> Avatars {
        Avatars { store: Store::default(), db: Db::default() }
    }

    pub async fn upload(&self, user: u64, key: &str, body: Body, now: u64) -> Result<(), UploadError> {
        let _ = now;
        self.store.put(key, body).await.map_err(|_| UploadError::Storage)?;
        self.db.commit_avatar(user, key).await?;
        Ok(())
    }

    pub async fn sweep(&self, now: u64) -> usize {
        let _ = now;
        0
    }
}

impl Default for Avatars {
    fn default() -> Self {
        Avatars::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::Ordering;
    use tokio::time::{Duration, timeout};

    fn body(data: &[u8]) -> Body {
        futures::stream::iter(vec![Ok(Bytes::copy_from_slice(data))]).boxed()
    }

    #[tokio::test]
    async fn a_normal_upload_sets_the_avatar_and_leaves_nothing_pending() {
        let a = Avatars::new();
        a.upload(1, "k1", body(b"first"), 100).await.unwrap();
        assert_eq!(a.db.avatar_of(1).as_deref(), Some("k1"), "ссылка записана");
        assert_eq!(a.store.keys(), vec!["k1".to_string()], "файл лежит");
        assert_eq!(a.db.pending_count(), 0, "после успеха записи «ожидает» нет");
    }

    #[tokio::test]
    async fn replacing_an_avatar_removes_the_old_file_only_after_the_commit() {
        let a = Avatars::new();
        a.upload(1, "k1", body(b"first"), 100).await.unwrap();
        a.upload(1, "k2", body(b"second"), 200).await.unwrap();
        assert_eq!(a.store.keys(), vec!["k2".to_string()], "старый файл убран, новый на месте");

        a.db.fail_commit.store(true, Ordering::SeqCst);
        assert_eq!(a.upload(1, "k3", body(b"third"), 300).await, Err(UploadError::Db), "база отказала");
        assert_eq!(a.db.avatar_of(1).as_deref(), Some("k2"), "аватар остался прежним");
        assert!(a.store.keys().contains(&"k2".to_string()), "старый файл не удаляют, пока новая ссылка не записана: иначе профиль остался бы без картинки");
    }

    #[tokio::test]
    async fn after_a_failed_commit_the_orphan_is_found_and_removed_by_the_sweep() {
        let a = Avatars::new();
        a.db.fail_commit.store(true, Ordering::SeqCst);
        assert!(a.upload(1, "orphan", body(b"data"), 1_000).await.is_err(), "база отказала");
        a.db.fail_commit.store(false, Ordering::SeqCst);
        assert_eq!(a.db.pending_count(), 1, "о незавершённой загрузке в базе осталась запись: иначе сироту нечем найти");
        assert_eq!(a.sweep(1_000 + GRACE + 1).await, 1, "sweep убрал одну сироту");
        assert!(a.store.keys().is_empty(), "файл сироты удалён");
        assert_eq!(a.db.pending_count(), 0, "и запись о нём тоже");
        assert_eq!(a.sweep(1_000 + GRACE + 2).await, 0, "повторный проход ничего не находит");
    }

    #[tokio::test]
    async fn a_process_that_dies_mid_upload_leaves_a_findable_orphan() {
        let a = Arc::new(Avatars::new());
        a.db.hold.store(true, Ordering::SeqCst);
        let task = tokio::spawn({
            let a = a.clone();
            async move { a.upload(7, "dead", body(b"bytes"), 5_000).await }
        });
        wait_for(|| a.store.keys() == vec!["dead".to_string()]).await;
        task.abort();
        let _ = timeout(Duration::from_secs(5), task).await.expect("задача завершилась");
        assert_eq!(a.db.avatar_of(7), None, "ссылка не записана, процесс умер до коммита");
        assert_eq!(a.sweep(5_000 + GRACE + 1).await, 1, "уборщик находит сироту по записи «ожидает», хотя за собой никто ничего не убрал");
        assert!(a.store.keys().is_empty(), "файл убран");
    }

    #[tokio::test]
    async fn the_sweep_does_not_touch_uploads_in_flight_or_finished_files() {
        let a = Arc::new(Avatars::new());
        a.upload(1, "done", body(b"ok"), 100).await.unwrap();
        a.db.hold.store(true, Ordering::SeqCst);
        let slow = tokio::spawn({
            let a = a.clone();
            async move { a.upload(2, "slow", body(b"in flight"), 10_000).await }
        });
        wait_for(|| a.store.keys().contains(&"slow".to_string())).await;
        assert_eq!(a.sweep(10_000 + GRACE - 1).await, 0, "загрузка, которой меньше GRACE секунд, ещё идёт: убирать её нельзя");
        assert_eq!(a.store.keys(), vec!["done".to_string(), "slow".to_string()], "оба файла на месте");
        a.db.hold.store(false, Ordering::SeqCst);
        a.db.hold_commit.notify_one();
        timeout(Duration::from_secs(5), slow).await.unwrap().unwrap().unwrap();
        assert_eq!(a.db.avatar_of(2).as_deref(), Some("slow"), "медленная загрузка завершилась успешно");
        assert_eq!(a.sweep(100_000).await, 0, "завершённые загрузки уборщик не трогает");
        assert_eq!(a.store.keys(), vec!["done".to_string(), "slow".to_string()], "и файлы живы");
    }

    #[tokio::test]
    async fn a_failing_body_cleans_up_after_itself() {
        let a = Avatars::new();
        let bad = futures::stream::iter(vec![Ok(Bytes::from_static(b"half")), Err(io::Error::new(io::ErrorKind::ConnectionReset, "обрыв"))]).boxed();
        assert_eq!(a.upload(1, "broken", bad, 100).await, Err(UploadError::Storage), "ошибка хранилища");
        assert!(a.store.keys().is_empty(), "обрыв не оставляет файла");
        assert_eq!(a.db.pending_count(), 0, "и записи «ожидает»: делать нечего");
    }

    pub async fn wait_for(mut check: impl FnMut() -> bool) {
        for _ in 0..500 {
            if check() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("не дождались условия");
    }
}
