
use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::time::Instant;
use tokio::sync::watch;

fn main() {}

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    InvalidArgument,
    KeyReuse,
    Storage,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Created {
    pub id: u64,
    pub replayed: bool,
}

struct Entry {
    text: String,
    id: u64,
    at: Instant,
}

enum Slot {
    InFlight { text: String, finished: watch::Receiver<()> },
    Done(Entry),
}

enum Step {
    Reply(Result<Created, Error>),
    Wait(watch::Receiver<()>),
    Claimed(watch::Sender<()>),
}

/// Пока создатель жив, ключ занят. Когда он закончил, отказал или был отменён, ключ освобождается, а ждущие просыпаются.
struct Claim<'a> {
    posts: &'a Posts,
    key: (u64, String),
    _tx: watch::Sender<()>,
}

impl Drop for Claim<'_> {
    fn drop(&mut self) {
        let mut inner = self.posts.inner.lock().unwrap();
        if matches!(inner.slots.get(&self.key), Some(Slot::InFlight { .. })) {
            inner.slots.remove(&self.key);
        }
    }
}

#[derive(Default)]
struct Inner {
    next_id: u64,
    posts: Vec<(u64, String)>,
    slots: HashMap<(u64, String), Slot>,
}

pub struct Posts {
    ttl: Duration,
    cost: Duration,
    fail_next: AtomicBool,
    inner: Mutex<Inner>,
}

impl Posts {
    /// `cost`: сколько занимает запись поста в базу.
    pub fn new(ttl: Duration, cost: Duration) -> Posts {
        Posts { ttl, cost, fail_next: AtomicBool::new(false), inner: Mutex::new(Inner::default()) }
    }

    pub fn count(&self) -> usize {
        self.inner.lock().unwrap().posts.len()
    }

    /// Следующая запись в базу закончится ошибкой.
    pub fn fail_next(&self) {
        self.fail_next.store(true, Ordering::SeqCst);
    }

    async fn store(&self) -> Result<(), Error> {
        tokio::time::sleep(self.cost).await;
        if self.fail_next.swap(false, Ordering::SeqCst) { Err(Error::Storage) } else { Ok(()) }
    }

    pub async fn create(&self, user: u64, key: Option<&str>, text: &str) -> Result<Created, Error> {
        if text.trim().is_empty() {
            return Err(Error::InvalidArgument);
        }
        let Some(key) = key else {
            self.store().await?;
            return Ok(self.insert(user, None, text));
        };
        let map_key = (user, key.to_string());
        loop {
            let step = {
                let mut inner = self.inner.lock().unwrap();
                match inner.slots.get(&map_key) {
                    Some(Slot::Done(found)) if Instant::now().duration_since(found.at) < self.ttl => {
                        Step::Reply(if found.text == text { Ok(Created { id: found.id, replayed: true }) } else { Err(Error::KeyReuse) })
                    }
                    Some(Slot::InFlight { text: other, finished }) => {
                        if other == text { Step::Wait(finished.clone()) } else { Step::Reply(Err(Error::KeyReuse)) }
                    }
                    _ => {
                        let (tx, rx) = watch::channel(());
                        inner.slots.insert(map_key.clone(), Slot::InFlight { text: text.to_string(), finished: rx });
                        Step::Claimed(tx)
                    }
                }
            };
            match step {
                Step::Reply(result) => return result,
                Step::Wait(mut finished) => {
                    let _ = finished.changed().await;
                }
                Step::Claimed(tx) => {
                    let _claim = Claim { posts: self, key: map_key.clone(), _tx: tx };
                    self.store().await?;
                    return Ok(self.insert(user, Some(key), text));
                }
            }
        }
    }

    fn insert(&self, user: u64, key: Option<&str>, text: &str) -> Created {
        let mut inner = self.inner.lock().unwrap();
        inner.next_id += 1;
        let id = inner.next_id;
        inner.posts.push((user, text.to_string()));
        if let Some(key) = key {
            inner.slots.insert((user, key.to_string()), Slot::Done(Entry { text: text.to_string(), id, at: Instant::now() }));
        }
        Created { id, replayed: false }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::future::join_all;
    use std::sync::Arc;

    const DAY: Duration = Duration::from_secs(24 * 3600);
    const COST: Duration = Duration::from_millis(50);

    #[tokio::test(start_paused = true)]
    async fn simultaneous_requests_with_one_key_create_one_post() {
        let posts = Arc::new(Posts::new(DAY, COST));
        let start = Instant::now();
        let results = join_all((0..8).map(|_| {
            let p = posts.clone();
            async move { p.create(1, Some("k"), "привет").await }
        }))
        .await;
        let ok: Vec<Created> = results.into_iter().map(|r| r.expect("все получают результат")).collect();
        assert_eq!(posts.count(), 1, "восемь одновременных запросов с одним ключом создали {} постов", posts.count());
        assert!(ok.iter().all(|c| c.id == ok[0].id), "все восемь должны получить один и тот же id");
        assert_eq!(ok.iter().filter(|c| !c.replayed).count(), 1, "создателем должен быть ровно один запрос");
        assert_eq!(start.elapsed(), COST, "остальные ждут результата создателя, а не пишут в базу сами");
    }

    #[tokio::test(start_paused = true)]
    async fn different_keys_are_still_parallel() {
        let posts = Arc::new(Posts::new(DAY, COST));
        let start = Instant::now();
        let results = join_all((0..4).map(|i| {
            let p = posts.clone();
            async move { p.create(1, Some(&format!("k{i}")), "текст").await }
        }))
        .await;
        assert!(results.iter().all(|r| r.is_ok()));
        assert_eq!(posts.count(), 4);
        assert_eq!(start.elapsed(), COST, "четыре разных ключа заняли {:?}: одна общая блокировка на всё время записи", start.elapsed());
    }

    #[tokio::test(start_paused = true)]
    async fn if_the_creator_fails_the_key_is_released() {
        let posts = Arc::new(Posts::new(DAY, COST));
        posts.fail_next();
        let results = tokio::time::timeout(
            Duration::from_secs(5),
            join_all((0..3).map(|_| {
                let p = posts.clone();
                async move { p.create(1, Some("k"), "текст").await }
            })),
        )
        .await
        .expect("запросы зависли: ключ остался занятым после ошибки создателя");
        let errors = results.iter().filter(|r| matches!(r, Err(Error::Storage))).count();
        assert_eq!(errors, 1, "ошибку получает только создатель");
        assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 2);
        assert_eq!(posts.count(), 1, "кто-то из ждавших должен был создать пост сам, и ровно один");
    }

    #[tokio::test(start_paused = true)]
    async fn a_cancelled_creator_does_not_block_the_key() {
        let posts = Arc::new(Posts::new(DAY, COST));
        let p = posts.clone();
        let creator = tokio::spawn(async move { p.create(1, Some("k"), "текст").await });
        tokio::time::sleep(Duration::from_millis(10)).await;
        creator.abort();
        let _ = creator.await;
        let r = tokio::time::timeout(Duration::from_secs(5), posts.create(1, Some("k"), "текст"))
            .await
            .expect("ключ завис в «выполняется» после отмены создателя")
            .unwrap();
        assert!(!r.replayed);
        assert_eq!(posts.count(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn a_different_body_in_flight_is_rejected_at_once() {
        let posts = Arc::new(Posts::new(DAY, COST));
        let p = posts.clone();
        let first = tokio::spawn(async move { p.create(1, Some("k"), "один").await });
        tokio::time::sleep(Duration::from_millis(10)).await;
        let start = Instant::now();
        assert_eq!(posts.create(1, Some("k"), "другой").await, Err(Error::KeyReuse));
        assert_eq!(start.elapsed(), Duration::ZERO, "расхождение видно сразу, ждать создателя незачем");
        first.await.unwrap().unwrap();
        assert_eq!(posts.count(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn requests_without_a_key_do_not_dedupe() {
        let posts = Arc::new(Posts::new(DAY, COST));
        let results = join_all((0..3).map(|_| {
            let p = posts.clone();
            async move { p.create(1, None, "текст").await }
        }))
        .await;
        assert!(results.iter().all(|r| r.is_ok()));
        assert_eq!(posts.count(), 3);
    }
}
