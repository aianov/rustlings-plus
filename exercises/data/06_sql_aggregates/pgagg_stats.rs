// Статистика соцсети тремя запросами. Тест проверяет цифры, а не форму запроса, но ловит
// размножение строк и потерянные пустые случаи.
//  1. `profile_counters`: $1 это id пользователя. Одна строка со столбцами followers (сколько
//     подписано на него), following (на сколько подписан он), posts (число его постов) и
//     likes_received (лайков на всех его постах). Все четыре числа считаются одним запросом,
//     и ни одно не должно искажаться от того, что соседние связи тоже большие.
//  2. `busy_authors`: авторы, у которых не меньше двух постов и не меньше трёх полученных лайков.
//     Столбцы username, posts, likes; порядок по нику.
//  3. `daily_posts`: посты по дням с 28 сентября по 3 октября 2026 включительно, с нулями для
//     пустых дней. Столбцы day (дата) и posts.
// TODO: Напиши три запроса.

fn profile_counters() -> &'static str {
    "SELECT 1"
}

fn busy_authors() -> &'static str {
    "SELECT 1"
}

fn daily_posts() -> &'static str {
    "SELECT 1"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    async fn counters(db: &TestDb, user: i64) -> (i64, i64, i64, i64) {
        sqlx::query_as::<_, (i64, i64, i64, i64)>(profile_counters())
            .bind(user)
            .fetch_one(&db.pool)
            .await
            .unwrap_or_else(|e| panic!("profile_counters({user}): {e}"))
    }

    #[tokio::test]
    async fn counters_do_not_multiply() {
        let db = TestDb::with_sql(SEED).await;
        assert_eq!(counters(&db, 1).await, (3, 2, 3, 8), "alice: 3 подписчика, 2 подписки, 3 поста, 8 лайков; цифры не должны перемножаться");
        assert_eq!(counters(&db, 2).await, (2, 1, 2, 3), "bob: 2 подписчика, 1 подписка, 2 поста, 3 лайка");
    }

    #[tokio::test]
    async fn counters_of_a_quiet_user() {
        let db = TestDb::with_sql(SEED).await;
        assert_eq!(counters(&db, 5).await, (0, 1, 0, 0), "у eve нет ни постов, ни подписчиков: нули, а не пропавшая строка или NULL");
    }

    #[tokio::test]
    async fn busy_authors_need_both_conditions() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(busy_authors()).await;
        assert_eq!(t.columns, ["username", "posts", "likes"], "нужны столбцы username, posts, likes");
        assert_eq!(
            t.rows,
            [["alice", "3", "8"], ["bob", "2", "3"]],
            "carol пишет часто, но лайков не получала; dave лайков ждёт и постов мало:\n{t}"
        );
    }

    #[tokio::test]
    async fn every_day_present() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(daily_posts()).await;
        assert_eq!(t.columns, ["day", "posts"], "нужны столбцы day и posts");
        assert_eq!(
            t.rows,
            [["2026-09-28", "2"], ["2026-09-29", "1"], ["2026-09-30", "0"], ["2026-10-01", "2"], ["2026-10-02", "0"], ["2026-10-03", "3"]],
            "шесть дней, в пустые стоит 0 (а не 1 и не пропуск):\n{t}"
        );
    }
}
