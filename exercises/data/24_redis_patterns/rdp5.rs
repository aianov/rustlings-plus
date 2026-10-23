// Лимит «60 запросов в минуту» сделан счётчиком: INCR, и если это первый запрос в окне, EXPIRE на 60 секунд.
// После падения сервиса между INCR и EXPIRE у некоторых пользователей ключ лимита остался без срока: счётчик
// только растёт, и через минуту пользователь заблокирован навсегда. Почини `hit` так, чтобы ключ лимита
// всегда имел срок, даже если он остался без срока после прошлого сбоя. Возвращай true, если запрос разрешён.

use course_support::redis::{Client, Error};

async fn hit(c: &Client, ns: &str, user: i64, limit: i64) -> Result<bool, Error> {
    let key = format!("{ns}:limit:{user}");
    let n = c.cmd(&["INCR", &key]).await?.int().unwrap_or(0);
    if n == 1 {
        c.cmd(&["EXPIRE", &key, "60"]).await?;
    }
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
