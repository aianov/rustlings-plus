// Под вирусным постом лайки сыплются по десять тысяч в секунду, и каждый превращается в отдельную запись в базу: база захлёбывается
// на одной строке, остальные запросы ждут. Кроме того, при остановке сервиса задачи с записями, не успевшие выполниться, просто
// пропадают вместе с рантаймом, и лайки теряются. Счётчик лайков не обязан быть точным ежесекундно, зато он обязан быть точным в итоге.
// Цель: лайки копятся в памяти, раз в `interval` по каждому посту в базу уходит одна сумма. Лайк, пришедший во время записи,
// не теряется. `close` останавливает сброс и дописывает накопленное, не дожидаясь очередного срока.


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
    pending: Mutex<Vec<tokio::task::JoinHandle<()>>>,
}

impl Likes {
    pub fn start(db: Arc<Db>, interval: Duration) -> Likes {
        let _ = interval;
        Likes { db, pending: Mutex::new(Vec::new()) }
    }

    /// Учитывает лайк. Не ждёт базы.
    pub fn like(&self, post: u64) {
        let db = self.db.clone();
        let handle = tokio::spawn(async move { db.add(post, 1).await });
        self.pending.lock().unwrap().push(handle);
    }

    /// Останавливает сброс и записывает всё накопленное.
    pub async fn close(&self) {
        let handles: Vec<_> = self.pending.lock().unwrap().drain(..).collect();
        for h in handles {
            h.abort();
        }
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
