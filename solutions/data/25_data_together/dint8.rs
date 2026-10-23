use course_support::redis::{Client, Error};

const APPLY: &str = "
local current = tonumber(redis.call('HGET', KEYS[1], 'version') or '-1')
if tonumber(ARGV[1]) <= current then
  return 0
end
redis.call('HSET', KEYS[1], 'version', ARGV[1], 'data', ARGV[2])
redis.call('EXPIRE', KEYS[1], 3600)
return 1
";

async fn on_profile_changed(c: &Client, ns: &str, user: i64, version: i64, data: &str) -> Result<bool, Error> {
    let key = format!("{ns}:profile:{user}");
    Ok(c.cmd(&["EVAL", APPLY, "1", &key, &version.to_string(), data]).await?.int() == Some(1))
}

async fn cached(c: &Client, ns: &str, user: i64) -> Result<Option<String>, Error> {
    Ok(c.cmd(&["HGET", &format!("{ns}:profile:{user}"), "data"]).await?.text())
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::redis::Scope;

    #[tokio::test]
    async fn cache_never_goes_back() {
        let s = Scope::new().await;
        let c = s.client();
        assert!(on_profile_changed(&c, s.prefix(), 7, 2, "{\"name\":\"kate v2\"}").await.expect("v2"));
        assert!(!on_profile_changed(&c, s.prefix(), 7, 1, "{\"name\":\"kate v1\"}").await.expect("v1 опоздала"), "старая версия не применяется");
        assert!(!on_profile_changed(&c, s.prefix(), 7, 2, "{\"name\":\"kate v2\"}").await.expect("повтор v2"), "повтор не нужен");
        assert_eq!(cached(&c, s.prefix(), 7).await.expect("кэш").as_deref(), Some("{\"name\":\"kate v2\"}"));
        assert!(on_profile_changed(&c, s.prefix(), 7, 3, "{\"name\":\"kate v3\"}").await.expect("v3"));
        assert_eq!(cached(&c, s.prefix(), 7).await.expect("кэш").as_deref(), Some("{\"name\":\"kate v3\"}"));
        let ttl = s.cmd(&["TTL", &s.key("profile:7")]).await.expect("TTL").int().unwrap_or(-1);
        assert!((1..=3600).contains(&ttl), "срок кэша час, TTL = {ttl}");
        assert_eq!(cached(&c, s.prefix(), 8).await.expect("кэш"), None);
    }
}
