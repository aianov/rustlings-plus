// База данных на секунду потеряла связь. В цепочке шлюз, post-service, user-service каждый слой «на всякий случай» повторяет вызов
// нижнего три раза. Один запрос пользователя превращается в двадцать семь обращений к базе, которой и так плохо, и она уже не
// встаёт: нагрузка от повторов больше, чем была от самих запросов. Это называется шторм повторов.
// Цель: система по-прежнему переживает короткий сбой базы (две неудачи подряд и успех), но на лежащую базу уходит не больше
// трёх обращений на запрос пользователя, сколько бы слоёв ни стояло в цепочке.


use std::future::Future;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

fn main() {}

#[derive(Debug, PartialEq, Eq)]
pub struct Status(pub &'static str);

pub struct Db {
    calls: AtomicU32,
    failures_left: AtomicU32,
}

impl Db {
    /// База, которая ответит ошибкой на первые `failures` обращений, а потом выздоровеет. `u32::MAX` значит «лежит».
    pub fn failing(failures: u32) -> Db {
        Db { calls: AtomicU32::new(0), failures_left: AtomicU32::new(failures) }
    }

    pub fn calls(&self) -> u32 {
        self.calls.load(Ordering::SeqCst)
    }

    async fn query(&self) -> Result<u64, Status> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let left = self.failures_left.load(Ordering::SeqCst);
        if left > 0 {
            if left != u32::MAX {
                self.failures_left.store(left - 1, Ordering::SeqCst);
            }
            return Err(Status("unavailable"));
        }
        Ok(42)
    }
}

/// Повторяет `f` до `attempts` раз всего, с паузой в 10 мс.
async fn retrying<T, Fut: Future<Output = Result<T, Status>>>(attempts: u32, mut f: impl FnMut() -> Fut) -> Result<T, Status> {
    let mut last = Status("no attempts");
    for n in 0..attempts {
        if n > 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        match f().await {
            Ok(v) => return Ok(v),
            Err(e) => last = e,
        }
    }
    Err(last)
}

async fn user_service(db: &Db) -> Result<u64, Status> {
    retrying(3, || db.query()).await
}

async fn post_service(db: &Db) -> Result<String, Status> {
    let user = retrying(3, || user_service(db)).await?;
    Ok(format!("посты пользователя {user}"))
}

pub async fn gateway(db: &Db) -> Result<String, Status> {
    retrying(3, || post_service(db)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn a_dead_database_gets_at_most_three_calls_per_request() {
        let db = Db::failing(u32::MAX);
        assert_eq!(gateway(&db).await, Err(Status("unavailable")));
        assert!(db.calls() <= 3, "на одну лежащую базу ушло {} обращений от одного запроса: повторы умножаются по слоям", db.calls());
        assert!(db.calls() >= 2, "повторов нет совсем: короткий сбой теперь убьёт запрос");
    }

    #[tokio::test(start_paused = true)]
    async fn a_short_outage_is_still_survived() {
        let db = Db::failing(2);
        assert_eq!(gateway(&db).await.as_deref(), Ok("посты пользователя 42"), "две неудачи подряд и успех: запрос обязан пережить это");
        assert_eq!(db.calls(), 3);
    }

    #[tokio::test(start_paused = true)]
    async fn a_longer_outage_still_fails() {
        let db = Db::failing(3);
        assert_eq!(gateway(&db).await, Err(Status("unavailable")), "три неудачи подряд исчерпывают попытки");
    }

    #[tokio::test(start_paused = true)]
    async fn a_hundred_requests_do_not_become_thousands() {
        let db = Db::failing(u32::MAX);
        for _ in 0..100 {
            let _ = gateway(&db).await;
        }
        assert!(db.calls() <= 300, "сто запросов принесли {} обращений к базе: ждали не больше трёхсот", db.calls());
    }
}
