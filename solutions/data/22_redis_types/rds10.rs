use course_support::redis::{Client, Error};

fn key(ns: &str) -> String {
    format!("{ns}:leaders:week")
}

async fn add_points(c: &Client, ns: &str, user: i64, points: i64) -> Result<i64, Error> {
    c.cmd(&["ZINCRBY", &key(ns), &points.to_string(), &user.to_string()])
        .await?
        .text()
        .and_then(|t| t.parse::<f64>().ok())
        .map(|f| f as i64)
        .ok_or_else(|| Error::Protocol("ZINCRBY вернул не число".into()))
}

async fn place(c: &Client, ns: &str, user: i64) -> Result<Option<i64>, Error> {
    Ok(c.cmd(&["ZREVRANK", &key(ns), &user.to_string()]).await?.int().map(|r| r + 1))
}

async fn drop_below(c: &Client, ns: &str, min: i64) -> Result<i64, Error> {
    let bound = format!("({min}");
    c.cmd(&["ZREMRANGEBYSCORE", &key(ns), "-inf", &bound])
        .await?
        .int()
        .ok_or_else(|| Error::Protocol("ZREMRANGEBYSCORE вернул не число".into()))
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::redis::Scope;

    #[tokio::test]
    async fn leaderboard_flow() {
        let s = Scope::new().await;
        let c = s.client();
        assert_eq!(add_points(&c, s.prefix(), 1, 30).await.expect("очки"), 30);
        assert_eq!(add_points(&c, s.prefix(), 1, 15).await.expect("очки"), 45, "очки прибавляются");
        add_points(&c, s.prefix(), 2, 60).await.expect("очки");
        add_points(&c, s.prefix(), 3, 10).await.expect("очки");
        add_points(&c, s.prefix(), 4, 9).await.expect("очки");
        add_points(&c, s.prefix(), 5, 2).await.expect("очки");
        assert_eq!(place(&c, s.prefix(), 2).await.expect("место"), Some(1), "у лидера первое место");
        assert_eq!(place(&c, s.prefix(), 1).await.expect("место"), Some(2));
        assert_eq!(place(&c, s.prefix(), 42).await.expect("место"), None, "кого нет в таблице, у того нет места");
        assert_eq!(drop_below(&c, s.prefix(), 10).await.expect("чистка"), 2, "удалены двое со счётом меньше 10, а ровно 10 остаётся");
        assert_eq!(place(&c, s.prefix(), 3).await.expect("место"), Some(3), "пользователь с ровно 10 очками остался");
    }
}
