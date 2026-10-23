// Блокировка с истечением не гарантирует, что работает один: процесс может зависнуть (пауза сборщика мусора,
// своп) дольше срока, блокировку заберёт другой, а первый проснётся и запишет результат поверх. Две защиты.
// Продление: пока работа идёт, владелец продлевает свою блокировку (только свою). Fencing token: при захвате
// выдаётся растущий номер (INCR счётчика), и хранилище принимает запись, только если номер больше последнего
// принятого. Хранилище здесь это ключ отчёта с полями fence и data в хеше.
// TODO: `acquire` (токен и номер), `renew`, `write_guarded`.

use course_support::redis::{Client, Error};
use uuid::Uuid;

struct Lease {
    token: String,
    fence: i64,
}

async fn acquire(c: &Client, key: &str, ttl_ms: u64) -> Result<Option<Lease>, Error> {
    let _ = (c, key, ttl_ms, Uuid::new_v4());
    todo!("SET NX PX и номер из INCR")
}

async fn renew(c: &Client, key: &str, token: &str, ttl_ms: u64) -> Result<bool, Error> {
    let _ = (c, key, token, ttl_ms);
    todo!("продлить только свою")
}

async fn write_guarded(c: &Client, store: &str, fence: i64, data: &str) -> Result<bool, Error> {
    let _ = (c, store, fence, data);
    todo!("записать, только если номер больше последнего")
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::redis::Scope;

    use std::time::Duration;

    #[tokio::test]
    async fn renewal_keeps_the_lock() {
        let s = Scope::new().await;
        let c = s.client();
        let key = s.key("lock:report");
        let a = acquire(&c, &key, 150).await.expect("acquire").expect("A взял");
        for _ in 0..4 {
            tokio::time::sleep(Duration::from_millis(80)).await;
            assert!(renew(&c, &key, &a.token, 150).await.expect("renew"), "владелец продлевает свою блокировку");
        }
        assert!(acquire(&c, &key, 150).await.expect("acquire").is_none(), "продлённую блокировку никто не взял");
        assert!(!renew(&c, &key, "чужой токен", 150).await.expect("renew"), "чужой не продлевает");
    }

    #[tokio::test]
    async fn stale_writer_is_fenced_off() {
        let s = Scope::new().await;
        let c = s.client();
        let (key, store) = (s.key("lock:report"), s.key("report"));
        let a = acquire(&c, &key, 100).await.expect("acquire").expect("A взял");
        tokio::time::sleep(Duration::from_millis(200)).await;
        let b = acquire(&c, &key, 5000).await.expect("acquire").expect("B взял после истечения");
        assert!(b.fence > a.fence, "номер растёт с каждым захватом");
        assert!(write_guarded(&c, &store, b.fence, "отчёт B").await.expect("запись"), "B пишет со свежим номером");
        assert!(!write_guarded(&c, &store, a.fence, "отчёт A").await.expect("запись"), "проснувшийся A со старым номером отвергнут");
        assert_eq!(s.cmd(&["HGET", &store, "data"]).await.expect("HGET").text().as_deref(), Some("отчёт B"));
    }
}
