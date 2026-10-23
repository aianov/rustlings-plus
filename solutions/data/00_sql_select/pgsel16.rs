fn users_from_older_than() -> &'static str {
    "SELECT username FROM users WHERE country = $1 AND age > $2 ORDER BY id"
}

fn posts_page() -> &'static str {
    "SELECT id FROM posts ORDER BY id LIMIT $1 OFFSET $2"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    async fn users(country: &str, age: i32) -> Vec<String> {
        let db = TestDb::with_sql(SEED).await;
        assert!(users_from_older_than().contains("$1"), "значения передаются метками $1 и $2, а не вклеиваются");
        sqlx::query_scalar::<_, String>(users_from_older_than())
            .bind(country)
            .bind(age)
            .fetch_all(&db.pool)
            .await
            .unwrap_or_else(|e| panic!("запрос с двумя параметрами не выполнился: {e}"))
    }

    async fn page(limit: i64, offset: i64) -> Vec<i64> {
        let db = TestDb::with_sql(SEED).await;
        sqlx::query_scalar::<_, i64>(posts_page())
            .bind(limit)
            .bind(offset)
            .fetch_all(&db.pool)
            .await
            .unwrap_or_else(|e| panic!("запрос с двумя параметрами не выполнился: {e}"))
    }

    #[tokio::test]
    async fn filter_with_params() {
        assert_eq!(users("DE", 30).await, ["carol", "Karl"], "из DE старше 30: carol и Karl");
        assert_eq!(users("US", 24).await, ["bob", "eve", "judy"], "из US старше 24: bob, eve и judy");
    }

    #[tokio::test]
    async fn injection_is_just_a_string() {
        assert!(users("DE' OR '1'='1", 0).await.is_empty(), "страны с таким названием нет, ответ должен быть пустым");
    }

    #[tokio::test]
    async fn paging_with_params() {
        assert_eq!(page(3, 0).await, [1, 2, 3], "первая страница по три поста");
        assert_eq!(page(2, 4).await, [5, 6], "две записи после пропуска четырёх");
    }
}
