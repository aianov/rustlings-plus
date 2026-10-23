use course_support::redis::{Client, Error};
use uuid::Uuid;

const UNLOCK: &str = "
if redis.call('GET', KEYS[1]) == ARGV[1] then
  return redis.call('DEL', KEYS[1])
end
return 0
";

async fn lock(c: &Client, key: &str, ttl_ms: u64) -> Result<Option<String>, Error> {
    let token = Uuid::new_v4().to_string();
    let reply = c.cmd(&["SET", key, &token, "NX", "PX", &ttl_ms.to_string()]).await?;
    Ok((!reply.is_nil()).then_some(token))
}

async fn unlock(c: &Client, key: &str, token: &str) -> Result<bool, Error> {
    Ok(c.cmd(&["EVAL", UNLOCK, "1", key, token]).await?.int() == Some(1))
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::redis::Scope;

    use std::time::Duration;

    #[tokio::test]
    async fn expired_owner_cannot_unlock_the_new_one() {
        let s = Scope::new().await;
        let c = s.client();
        let key = s.key("lock:report");
        let a = lock(&c, &key, 100).await.expect("lock").expect("A взял блокировку");
        assert!(lock(&c, &key, 100).await.expect("lock").is_none(), "пока A держит, второй не берёт");
        tokio::time::sleep(Duration::from_millis(200)).await;
        let b = lock(&c, &key, 5000).await.expect("lock").expect("после истечения B взял блокировку");
        assert!(!unlock(&c, &key, &a).await.expect("unlock"), "A не может снять блокировку B");
        assert_eq!(s.cmd(&["GET", &key]).await.expect("GET").text(), Some(b.clone()), "блокировка B на месте");
        assert!(unlock(&c, &key, &b).await.expect("unlock"), "B снимает свою");
        assert!(s.cmd(&["GET", &key]).await.expect("GET").is_nil(), "ключа больше нет");
    }
}
