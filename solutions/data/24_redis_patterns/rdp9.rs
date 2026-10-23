use course_support::redis::{Client, Error};
use uuid::Uuid;

struct Lease {
    token: String,
    fence: i64,
}

const RENEW: &str = "
if redis.call('GET', KEYS[1]) == ARGV[1] then
  return redis.call('PEXPIRE', KEYS[1], ARGV[2])
end
return 0
";

const GUARDED: &str = "
local last = tonumber(redis.call('HGET', KEYS[1], 'fence') or '0')
if tonumber(ARGV[1]) <= last then
  return 0
end
redis.call('HSET', KEYS[1], 'fence', ARGV[1], 'data', ARGV[2])
return 1
";

async fn acquire(c: &Client, key: &str, ttl_ms: u64) -> Result<Option<Lease>, Error> {
    let token = Uuid::new_v4().to_string();
    if c.cmd(&["SET", key, &token, "NX", "PX", &ttl_ms.to_string()]).await?.is_nil() {
        return Ok(None);
    }
    let fence = c.cmd(&["INCR", &format!("{key}:fence")]).await?.int().ok_or_else(|| Error::Protocol("INCR вернул не число".into()))?;
    Ok(Some(Lease { token, fence }))
}

async fn renew(c: &Client, key: &str, token: &str, ttl_ms: u64) -> Result<bool, Error> {
    Ok(c.cmd(&["EVAL", RENEW, "1", key, token, &ttl_ms.to_string()]).await?.int() == Some(1))
}

async fn write_guarded(c: &Client, store: &str, fence: i64, data: &str) -> Result<bool, Error> {
    Ok(c.cmd(&["EVAL", GUARDED, "1", store, &fence.to_string(), data]).await?.int() == Some(1))
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
