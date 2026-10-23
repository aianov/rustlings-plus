// Таблица лидеров недели: пользователи получают очки за активность, видят своё место, а в конце дня из
// таблицы выбрасываются те, у кого меньше 10 очков, чтобы она не росла бесконечно.
// TODO: `add_points` прибавляет очки и возвращает новый счёт, `place` возвращает место пользователя
// (1 у лидера, None, если его нет в таблице), `drop_below(min)` удаляет всех со счётом строго меньше `min`
// и возвращает, сколько удалено.

use course_support::redis::{Client, Error};

async fn add_points(c: &Client, ns: &str, user: i64, points: i64) -> Result<i64, Error> {
    let _ = (c, ns, user, points);
    todo!("ZINCRBY")
}

async fn place(c: &Client, ns: &str, user: i64) -> Result<Option<i64>, Error> {
    let _ = (c, ns, user);
    todo!("ZREVRANK")
}

async fn drop_below(c: &Client, ns: &str, min: i64) -> Result<i64, Error> {
    let _ = (c, ns, min);
    todo!("ZREMRANGEBYSCORE")
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
