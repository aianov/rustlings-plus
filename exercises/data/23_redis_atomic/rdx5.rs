// Перевод очков «только если хватает» сделан чтением и двумя командами. Под нагрузкой баланс уходит в минус:
// несколько переводов читают один и тот же баланс. Транзакция MULTI здесь не поможет: внутри неё нельзя
// прочитать баланс и решить. Нужен скрипт на Lua: Redis выполняет его целиком, ни с чем не смешивая, а внутри
// можно и читать, и решать. Ключи передаются скрипту списком KEYS, а значения списком ARGV, текст скрипта
// не собирается из данных. Скрипт возвращает новый баланс отправителя или -1, если очков не хватает.

use course_support::redis::{Client, Error};
use std::time::Duration;

async fn transfer(c: &Client, from: &str, to: &str, amount: i64) -> Result<i64, Error> {
    let balance = c.cmd(&["GET", from]).await?.int().unwrap_or(0);
    if balance < amount {
        return Ok(-1);
    }
    tokio::time::sleep(Duration::from_millis(5)).await;
    let n = amount.to_string();
    let left = c.cmd(&["DECRBY", from, &n]).await?.int().unwrap_or(0);
    c.cmd(&["INCRBY", to, &n]).await?;
    Ok(left)
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
