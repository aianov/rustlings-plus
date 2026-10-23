use course_support::redis::{Client, Error};

const BUCKET: &str = "
local now = tonumber(ARGV[1])
local capacity = tonumber(ARGV[2])
local rate = tonumber(ARGV[3])
local state = redis.call('HMGET', KEYS[1], 'tokens', 'ts')
local tokens = tonumber(state[1]) or capacity
local ts = tonumber(state[2]) or now
if now > ts then
  tokens = math.min(capacity, tokens + (now - ts) * rate / 1000)
  ts = now
end
local ok = 0
if tokens >= 1 then
  tokens = tokens - 1
  ok = 1
end
redis.call('HSET', KEYS[1], 'tokens', tostring(tokens), 'ts', ts)
redis.call('PEXPIRE', KEYS[1], math.ceil(capacity * 1000 / rate) + 1000)
return ok
";

async fn take(c: &Client, ns: &str, user: i64, now_ms: i64, capacity: i64, per_sec: i64) -> Result<bool, Error> {
    let key = format!("{ns}:bucket:{user}");
    let reply = c.cmd(&["EVAL", BUCKET, "1", &key, &now_ms.to_string(), &capacity.to_string(), &per_sec.to_string()]).await?;
    Ok(reply.int() == Some(1))
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::redis::Scope;

    #[tokio::test]
    async fn burst_then_steady_rate() {
        let s = Scope::new().await;
        let c = s.client();
        let ns = s.prefix();
        let mut got = Vec::new();
        for t in [0, 0, 0, 0, 500, 1000, 1000, 5000, 5000, 5000, 5000] {
            got.push(take(&c, ns, 7, t, 3, 1).await.expect("ведро"));
        }
        assert_eq!(
            got,
            [true, true, true, false, false, true, false, true, true, true, false],
            "всплеск до трёх, потом жетон в секунду, а за долгую паузу не больше вместимости"
        );
    }

    #[tokio::test]
    async fn concurrent_takes_are_counted() {
        let s = Scope::new().await;
        let mut tasks = Vec::new();
        for _ in 0..10 {
            let (c, ns) = (s.client(), s.prefix().to_string());
            tasks.push(tokio::spawn(async move { take(&c, &ns, 9, 100, 3, 1).await.expect("ведро") }));
        }
        let mut ok = 0;
        for t in tasks {
            if t.await.expect("задача") {
                ok += 1;
            }
        }
        assert_eq!(ok, 3, "из десяти одновременных жетоны получили трое");
    }
}
