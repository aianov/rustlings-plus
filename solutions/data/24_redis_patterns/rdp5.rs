use course_support::redis::{Client, Error};

const HIT: &str = "
local n = redis.call('INCR', KEYS[1])
if redis.call('TTL', KEYS[1]) < 0 then
  redis.call('EXPIRE', KEYS[1], ARGV[1])
end
return n
";

async fn hit(c: &Client, ns: &str, user: i64, limit: i64) -> Result<bool, Error> {
    let key = format!("{ns}:limit:{user}");
    let n = c.cmd(&["EVAL", HIT, "1", &key, "60"]).await?.int().unwrap_or(0);
    Ok(n <= limit)
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::redis::Scope;

    #[tokio::test]
    async fn limit_key_always_expires() {
        let s = Scope::new().await;
        let c = s.client();
        let key = s.key("limit:7");
        s.cmd(&["SET", &key, "5"]).await.expect("ключ без срока после старого сбоя");
        assert!(hit(&c, s.prefix(), 7, 60).await.expect("запрос"), "шестой запрос в окне разрешён");
        let ttl = s.cmd(&["TTL", &key]).await.expect("TTL").int().unwrap_or(-1);
        assert!((1..=60).contains(&ttl), "у ключа лимита снова есть срок, TTL = {ttl}");
    }

    #[tokio::test]
    async fn limit_is_enforced() {
        let s = Scope::new().await;
        let c = s.client();
        let mut allowed = 0;
        for _ in 0..10 {
            if hit(&c, s.prefix(), 9, 3).await.expect("запрос") {
                allowed += 1;
            }
        }
        assert_eq!(allowed, 3, "из десяти запросов при лимите 3 прошли три");
        let ttl = s.cmd(&["TTL", &s.key("limit:9")]).await.expect("TTL").int().unwrap_or(-1);
        assert!((1..=60).contains(&ttl), "окно со сроком, TTL = {ttl}");
    }
}
