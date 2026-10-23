use course_support::redis::{Client, Error};

fn key(ns: &str, user: i64) -> String {
    format!("{ns}:user:{user}:following")
}

async fn follow(c: &Client, ns: &str, who: i64, whom: i64) -> Result<(), Error> {
    c.cmd(&["SADD", &key(ns, who), &whom.to_string()]).await?;
    Ok(())
}

async fn is_following(c: &Client, ns: &str, who: i64, whom: i64) -> Result<bool, Error> {
    Ok(c.cmd(&["SISMEMBER", &key(ns, who), &whom.to_string()]).await?.int() == Some(1))
}

async fn common(c: &Client, ns: &str, a: i64, b: i64) -> Result<Vec<i64>, Error> {
    let mut ids: Vec<i64> = c.cmd(&["SINTER", &key(ns, a), &key(ns, b)]).await?.texts().iter().filter_map(|t| t.parse().ok()).collect();
    ids.sort_unstable();
    Ok(ids)
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::redis::Scope;

    #[tokio::test]
    async fn follows_and_common() {
        let s = Scope::new().await;
        let c = s.client();
        for (who, whom) in [(1, 10), (1, 20), (1, 30), (2, 30), (2, 10), (2, 40), (1, 10)] {
            follow(&c, s.prefix(), who, whom).await.expect("подписка");
        }
        assert!(is_following(&c, s.prefix(), 1, 20).await.expect("проверка"));
        assert!(!is_following(&c, s.prefix(), 2, 20).await.expect("проверка"), "2 не подписан на 20");
        let before = c.round_trips();
        assert_eq!(common(&c, s.prefix(), 1, 2).await.expect("общие"), [10, 30], "общие подписки по возрастанию");
        assert_eq!(c.round_trips() - before, 1, "пересечение считает сервер, одним обменом");
        let size = s.cmd(&["SCARD", &s.key("user:1:following")]).await.expect("SCARD").int();
        assert_eq!(size, Some(3), "повторная подписка не дублирует элемент");
    }
}
