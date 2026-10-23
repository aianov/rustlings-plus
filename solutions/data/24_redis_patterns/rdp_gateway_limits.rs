use course_support::redis::{Client, Error};
use uuid::Uuid;

const TWO_BUCKETS: &str = "
local now = tonumber(ARGV[1])
local function level(key, capacity, rate)
  local s = redis.call('HMGET', key, 'tokens', 'ts')
  local tokens = tonumber(s[1]) or capacity
  local ts = tonumber(s[2]) or now
  if now > ts then
    tokens = math.min(capacity, tokens + (now - ts) * rate / 1000)
  end
  return tokens
end
local u = level(KEYS[1], 5, 1)
local i = level(KEYS[2], 10, 2)
local ok = 0
if u >= 1 and i >= 1 then
  u = u - 1
  i = i - 1
  ok = 1
end
redis.call('HSET', KEYS[1], 'tokens', tostring(u), 'ts', now)
redis.call('HSET', KEYS[2], 'tokens', tostring(i), 'ts', now)
redis.call('PEXPIRE', KEYS[1], 6000)
redis.call('PEXPIRE', KEYS[2], 6000)
return ok
";

async fn allow(c: &Client, ns: &str, user: i64, ip: &str, now_ms: i64) -> Result<bool, Error> {
    let (ku, ki) = (format!("{ns}:bucket:user:{user}"), format!("{ns}:bucket:ip:{ip}"));
    Ok(c.cmd(&["EVAL", TWO_BUCKETS, "2", &ku, &ki, &now_ms.to_string()]).await?.int() == Some(1))
}

async fn login(c: &Client, ns: &str, user: i64) -> Result<String, Error> {
    let token = Uuid::new_v4().to_string();
    let session = format!("{ns}:session:{token}");
    let set = format!("{ns}:user:{user}:sessions");
    c.pipeline(&[&["SET", &session, &user.to_string(), "EX", "1800"], &["SADD", &set, &token], &["EXPIRE", &set, "1800"]]).await?;
    Ok(token)
}

async fn touch(c: &Client, ns: &str, token: &str) -> Result<Option<i64>, Error> {
    Ok(c.cmd(&["GETEX", &format!("{ns}:session:{token}"), "EX", "1800"]).await?.int())
}

async fn logout_all(c: &Client, ns: &str, user: i64) -> Result<(), Error> {
    let set = format!("{ns}:user:{user}:sessions");
    let keys: Vec<String> = c.cmd(&["SMEMBERS", &set]).await?.texts().iter().map(|t| format!("{ns}:session:{t}")).collect();
    let mut args = vec!["DEL", set.as_str()];
    args.extend(keys.iter().map(String::as_str));
    c.cmd(&args).await?;
    Ok(())
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::redis::Scope;

    #[tokio::test]
    async fn user_and_ip_limits_together() {
        let s = Scope::new().await;
        let c = s.client();
        let ns = s.prefix();
        let mut ann = Vec::new();
        for _ in 0..6 {
            ann.push(allow(&c, ns, 1, "10.0.0.1", 0).await.expect("лимит"));
        }
        assert_eq!(ann, [true, true, true, true, true, false], "у пользователя вместимость 5");
        let mut others = 0;
        for u in 2..=8 {
            if allow(&c, ns, u, "10.0.0.1", 0).await.expect("лимит") {
                others += 1;
            }
        }
        assert_eq!(others, 5, "у IP вместимость 10: пять уже потратила ann");
        assert!(allow(&c, ns, 2, "10.0.0.1", 1000).await.expect("лимит"), "через секунду IP пополнился на 2");
        let tokens = s.cmd(&["HGET", &s.key("bucket:user:1"), "tokens"]).await.expect("HGET").text();
        assert_eq!(tokens.as_deref(), Some("0"), "отказ из-за одного ведра не списывает жетон из другого");
    }

    #[tokio::test]
    async fn concurrent_requests() {
        let s = Scope::new().await;
        let mut tasks = Vec::new();
        for _ in 0..20 {
            let (c, ns) = (s.client(), s.prefix().to_string());
            tasks.push(tokio::spawn(async move { allow(&c, &ns, 7, "10.0.0.9", 0).await.expect("лимит") }));
        }
        let mut ok = 0;
        for t in tasks {
            if t.await.expect("задача") {
                ok += 1;
            }
        }
        assert_eq!(ok, 5, "из двадцати одновременных прошло пять");
    }

    #[tokio::test]
    async fn sessions_slide_and_die_together() {
        let s = Scope::new().await;
        let c = s.client();
        let before = c.round_trips();
        let a = login(&c, s.prefix(), 7).await.expect("вход");
        assert_eq!(c.round_trips() - before, 1, "вход одним обменом");
        let b = login(&c, s.prefix(), 7).await.expect("вход");
        s.cmd(&["EXPIRE", &s.key(&format!("session:{a}")), "5"]).await.expect("почти истекла");
        assert_eq!(touch(&c, s.prefix(), &a).await.expect("запрос"), Some(7));
        let ttl = s.cmd(&["TTL", &s.key(&format!("session:{a}"))]).await.expect("TTL").int().unwrap_or(-1);
        assert!(ttl > 1700, "активность продлевает сессию, TTL = {ttl}");
        logout_all(&c, s.prefix(), 7).await.expect("выход везде");
        assert_eq!(touch(&c, s.prefix(), &a).await.expect("запрос"), None);
        assert_eq!(touch(&c, s.prefix(), &b).await.expect("запрос"), None);
    }
}
