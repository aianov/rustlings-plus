use course_support::redis::{Client, Error};

const TRANSFER: &str = "
local balance = tonumber(redis.call('GET', KEYS[1]) or '0')
local amount = tonumber(ARGV[1])
if balance < amount then
  return -1
end
local left = redis.call('DECRBY', KEYS[1], amount)
redis.call('INCRBY', KEYS[2], amount)
return left
";

async fn transfer(c: &Client, from: &str, to: &str, amount: i64) -> Result<i64, Error> {
    c.cmd(&["EVAL", TRANSFER, "2", from, to, &amount.to_string()])
        .await?
        .int()
        .ok_or_else(|| Error::Protocol("скрипт вернул не число".into()))
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::redis::Scope;

    const SOURCE: &str = include_str!("rdx5.rs");

    #[tokio::test]
    async fn balance_never_goes_negative() {
        let s = Scope::new().await;
        let (a, b) = (s.key("points:a"), s.key("points:b"));
        s.cmd(&["MSET", &a, "100", &b, "0"]).await.expect("балансы");
        let mut tasks = Vec::new();
        for _ in 0..30 {
            let (c, a, b) = (s.client(), a.clone(), b.clone());
            tasks.push(tokio::spawn(async move { transfer(&c, &a, &b, 10).await.expect("перевод") }));
        }
        let mut ok = 0;
        for t in tasks {
            if t.await.expect("задача") >= 0 {
                ok += 1;
            }
        }
        assert_eq!(ok, 10, "из 30 переводов по 10 очков прошли ровно 10");
        assert_eq!(s.cmd(&["MGET", &a, &b]).await.expect("MGET").texts(), ["0", "100"], "баланс не ушёл в минус, очки не потерялись");
    }

    #[test]
    fn keys_and_values_are_passed_separately() {
        let code = SOURCE.split("#[cfg(test)]").next().unwrap_or_default();
        assert!(code.contains("KEYS[1]") && code.contains("ARGV[1]"), "скрипт получает ключи через KEYS и значения через ARGV");
        assert!(code.contains("\"EVAL\"") || code.contains("\"EVALSHA\""), "перевод выполняется скриптом");
    }
}
