
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn main() {}

#[derive(Default)]
pub struct Db {
    totals: Mutex<HashMap<u64, i64>>,
    writes: AtomicU32,
}

impl Db {
    /// Прибавляет к счётчику поста. Запись стоит 2 мс.
    pub async fn add(&self, post: u64, delta: i64) {
        self.writes.fetch_add(1, Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(2)).await;
        *self.totals.lock().unwrap().entry(post).or_default() += delta;
    }

    pub fn likes(&self, post: u64) -> i64 {
        self.totals.lock().unwrap().get(&post).copied().unwrap_or(0)
    }

    pub fn writes(&self) -> u32 {
        self.writes.load(Ordering::SeqCst)
    }
}

pub struct Likes {
    db: Arc<Db>,
    pending: Arc<Mutex<HashMap<u64, i64>>>,
    stop: Arc<tokio::sync::Notify>,
    flusher: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

async fn flush(db: &Db, pending: &Mutex<HashMap<u64, i64>>) {
    let batch = std::mem::take(&mut *pending.lock().unwrap());
    for (post, delta) in batch {
        db.add(post, delta).await;
    }
}

impl Likes {
    pub fn start(db: Arc<Db>, interval: Duration) -> Likes {
        let pending = Arc::new(Mutex::new(HashMap::new()));
        let stop = Arc::new(tokio::sync::Notify::new());
        let flusher = tokio::spawn({
            let (db, pending, stop) = (db.clone(), pending.clone(), stop.clone());
            async move {
                loop {
                    tokio::select! {
                        _ = tokio::time::sleep(interval) => flush(&db, &pending).await,
                        _ = stop.notified() => return,
                    }
                }
            }
        });
        Likes { db, pending, stop, flusher: Mutex::new(Some(flusher)) }
    }

    /// Учитывает лайк. Не ждёт базы.
    pub fn like(&self, post: u64) {
        *self.pending.lock().unwrap().entry(post).or_default() += 1;
    }

    /// Останавливает сброс и записывает всё накопленное.
    pub async fn close(&self) {
        self.stop.notify_one();
        let flusher = self.flusher.lock().unwrap().take();
        if let Some(f) = flusher {
            let _ = f.await;
        }
        flush(&self.db, &self.pending).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: fn(u64) -> Duration = Duration::from_millis;

    #[tokio::test(start_paused = true)]
    async fn ten_thousand_likes_become_a_few_writes() {
        let db = Arc::new(Db::default());
        let likes = Likes::start(db.clone(), MS(100));
        for i in 0..10_000u64 {
            likes.like(i % 3);
        }
        tokio::time::sleep(MS(150)).await;
        assert!(db.writes() <= 3, "10 000 лайков на три поста дали {} записей в базу: по одной записи на лайк", db.writes());
        assert_eq!((db.likes(0), db.likes(1), db.likes(2)), (3334, 3333, 3333));
        likes.close().await;
    }

    #[tokio::test(start_paused = true)]
    async fn likes_during_a_flush_are_not_lost() {
        let db = Arc::new(Db::default());
        let likes = Likes::start(db.clone(), MS(100));
        for _ in 0..50 {
            for _ in 0..40 {
                likes.like(7);
            }
            tokio::time::sleep(MS(1)).await;
        }
        likes.close().await;
        assert_eq!(db.likes(7), 2000, "после остановки в базе должны быть все лайки, а там {}", db.likes(7));
    }

    #[tokio::test(start_paused = true)]
    async fn close_does_not_wait_for_the_next_interval() {
        let db = Arc::new(Db::default());
        let likes = Likes::start(db.clone(), Duration::from_secs(3600));
        likes.like(1);
        likes.like(1);
        likes.like(2);
        let start = tokio::time::Instant::now();
        likes.close().await;
        assert_eq!((db.likes(1), db.likes(2)), (2, 1), "накопленное не записано при остановке");
        assert!(start.elapsed() < MS(100), "остановка ждала срока сброса {:?}", start.elapsed());
    }

    #[tokio::test(start_paused = true)]
    async fn no_likes_no_writes() {
        let db = Arc::new(Db::default());
        let likes = Likes::start(db.clone(), MS(100));
        tokio::time::sleep(Duration::from_secs(2)).await;
        likes.close().await;
        assert_eq!(db.writes(), 0);
    }
}
