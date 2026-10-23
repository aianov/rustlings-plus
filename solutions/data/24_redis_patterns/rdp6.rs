use course_support::redis::{Client, Error};
use uuid::Uuid;

const WINDOW: &str = "
redis.call('ZREMRANGEBYSCORE', KEYS[1], '-inf', tonumber(ARGV[1]) - tonumber(ARGV[2]))
if redis.call('ZCARD', KEYS[1]) >= tonumber(ARGV[3]) then
  return 0
end
redis.call('ZADD', KEYS[1], ARGV[1], ARGV[4])
redis.call('PEXPIRE', KEYS[1], ARGV[2])
return 1
";

async fn allow(c: &Client, ns: &str, user: i64, now_ms: i64, limit: i64, window_ms: i64) -> Result<bool, Error> {
    let key = format!("{ns}:rate:{user}");
    let member = format!("{now_ms}-{}", Uuid::new_v4());
    let reply = c.cmd(&["EVAL", WINDOW, "1", &key, &now_ms.to_string(), &window_ms.to_string(), &limit.to_string(), &member]).await?;
    Ok(reply.int() == Some(1))
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::redis::Scope;

    #[tokio::test]
    async fn sliding_window_has_no_boundary_burst() {
        let s = Scope::new().await;
        let c = s.client();
        let ns = s.prefix();
        let mut got = Vec::new();
        for t in [995, 996, 997, 998, 999, 1000, 1001, 1002, 1996, 1997] {
            got.push(allow(&c, ns, 7, t, 5, 1000).await.expect("лимит"));
        }
        assert_eq!(got, [true, true, true, true, true, false, false, false, true, true], "в любую секунду не больше 5, а через секунду после первых снова можно");
    }

    #[tokio::test]
    async fn concurrent_requests_respect_the_limit() {
        let s = Scope::new().await;
        let mut tasks = Vec::new();
        for _ in 0..20 {
            let (c, ns) = (s.client(), s.prefix().to_string());
            tasks.push(tokio::spawn(async move { allow(&c, &ns, 9, 5000, 5, 1000).await.expect("лимит") }));
        }
        let mut allowed = 0;
        for t in tasks {
            if t.await.expect("задача") {
                allowed += 1;
            }
        }
        assert_eq!(allowed, 5, "из двадцати одновременных прошло ровно пять");
    }
}
