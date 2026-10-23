fn follower_growth() -> &'static str {
    "SELECT created_at::date AS day, count(*) AS new_followers,
            sum(count(*)) OVER (ORDER BY created_at::date) AS total
     FROM follows
     WHERE followee_id = $1
     GROUP BY created_at::date
     ORDER BY day"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    async fn growth(db: &TestDb, user: i64) -> Vec<(String, i64, i64)> {
        sqlx::query_as::<_, (String, i64, i64)>(&format!("SELECT day::text, new_followers::bigint, total::bigint FROM ({}) AS q", follower_growth()))
            .bind(user)
            .fetch_all(&db.pool)
            .await
            .unwrap_or_else(|e| panic!("follower_growth({user}): {e}"))
    }

    fn row(day: &str, n: i64, total: i64) -> (String, i64, i64) {
        (day.to_string(), n, total)
    }

    #[tokio::test]
    async fn running_total_by_day() {
        let db = TestDb::with_sql(SEED).await;
        assert_eq!(
            growth(&db, 1).await,
            [row("2026-10-01", 2, 2), row("2026-10-02", 1, 3), row("2026-10-04", 3, 6)],
            "у alice за три дня 2, 1 и 3 новых подписчика; накопленное: 2, 3, 6 (дни без подписок в списке не нужны)"
        );
    }

    #[tokio::test]
    async fn other_users_and_empty() {
        let db = TestDb::with_sql(SEED).await;
        assert_eq!(growth(&db, 2).await, [row("2026-10-03", 1, 1), row("2026-10-04", 1, 2)], "у bob два подписчика в разные дни");
        assert!(growth(&db, 6).await.is_empty(), "у frank подписчиков нет");
    }
}
