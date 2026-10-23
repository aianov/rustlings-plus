// Знаменитость с ста тысячами подписчиков публикует пост. Обработчик запроса «опубликовать» честно раскладывает его по всем
// лентам, пачками по тысяче, и держит автора у ответа целую секунду (на настоящих десяти миллионах это были бы минуты). Пока он
// раскладывает, соседние публикации маленьких авторов стоят в очереди за этой, а при остановке сервиса недоделанное теряется.
// Цель: `publish` отвечает сразу. Раскладка идёт в фоне пачками, большие раскладки не мешают друг другу и маленьким, первые подписчики
// видят пост без ожидания конца, а `close` дожидается всех начатых раскладок, чтобы ничего не потерять при остановке.


use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn main() {}

const BATCH: usize = 1000;
/// Стоимость записи одной пачки в ленты.
const BATCH_COST: Duration = Duration::from_millis(10);

pub struct Service {
    followers: Mutex<HashMap<u64, Arc<Vec<u64>>>>,
    lines: Arc<Mutex<HashMap<u64, Vec<u64>>>>,
}

impl Service {
    pub fn new() -> Service {
        Service {
            followers: Mutex::new(HashMap::new()),
            lines: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn set_followers(&self, author: u64, followers: Vec<u64>) {
        self.followers.lock().unwrap().insert(author, Arc::new(followers));
    }

    /// Сколько лент содержат пост.
    pub fn delivered(&self, post: u64) -> usize {
        self.lines.lock().unwrap().values().filter(|l| l.contains(&post)).count()
    }

    pub async fn publish(&self, author: u64, post: u64) {
        let followers = self.followers.lock().unwrap().get(&author).cloned().unwrap_or_default();
        for batch in followers.chunks(BATCH) {
            {
                let mut lines = self.lines.lock().unwrap();
                for &user in batch {
                    lines.entry(user).or_default().push(post);
                }
            }
            tokio::time::sleep(BATCH_COST).await;
        }
    }

    /// Дожидается всех начатых раскладок.
    pub async fn close(&self) {
        let _ = &self.lines;
    }
}

impl Default for Service {
    fn default() -> Service {
        Service::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::Instant;

    const MS: fn(u64) -> Duration = Duration::from_millis;

    fn crowd(n: u64) -> Vec<u64> {
        (1..=n).collect()
    }

    #[tokio::test(start_paused = true)]
    async fn publish_returns_immediately() {
        let svc = Service::new();
        svc.set_followers(1, crowd(100_000));
        let start = Instant::now();
        svc.publish(1, 7).await;
        assert_eq!(start.elapsed(), Duration::ZERO, "автор ждал раскладку {:?}: она должна идти в фоне", start.elapsed());
    }

    #[tokio::test(start_paused = true)]
    async fn everyone_gets_the_post_eventually_without_duplicates() {
        let svc = Service::new();
        svc.set_followers(1, crowd(100_000));
        svc.publish(1, 7).await;
        tokio::time::sleep(Duration::from_secs(2)).await;
        assert_eq!(svc.delivered(7), 100_000);
        assert!(svc.lines.lock().unwrap().values().all(|l| l.iter().filter(|p| **p == 7).count() == 1));
    }

    #[tokio::test(start_paused = true)]
    async fn first_subscribers_see_it_before_the_job_ends() {
        let svc = Service::new();
        svc.set_followers(1, crowd(100_000));
        svc.publish(1, 7).await;
        tokio::time::sleep(MS(25)).await;
        let seen = svc.delivered(7);
        assert!((1000..100_000).contains(&seen), "через 25 мс должны быть записаны первые пачки, а не ноль и не всё: {seen}");
    }

    #[tokio::test(start_paused = true)]
    async fn a_small_author_is_not_stuck_behind_a_big_one() {
        let svc = Service::new();
        svc.set_followers(1, crowd(100_000));
        svc.set_followers(2, (200_000..200_050).collect());
        svc.publish(1, 7).await;
        svc.publish(2, 8).await;
        tokio::time::sleep(MS(15)).await;
        assert_eq!(svc.delivered(8), 50, "пост маленького автора должен дойти, не дожидаясь большой раскладки");
        assert!(svc.delivered(7) < 100_000);
    }

    #[tokio::test(start_paused = true)]
    async fn close_waits_for_started_jobs() {
        let svc = Service::new();
        svc.set_followers(1, crowd(30_000));
        svc.set_followers(2, (50_000..60_000).collect());
        svc.publish(1, 7).await;
        svc.publish(2, 8).await;
        svc.close().await;
        assert_eq!((svc.delivered(7), svc.delivered(8)), (30_000, 10_000), "при остановке начатые раскладки должны быть доведены до конца");
    }
}
