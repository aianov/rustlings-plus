use sqlx::PgPool;

async fn username_taken(pool: &PgPool, name: &str) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM users WHERE lower(username) = lower($1))")
        .bind(name)
        .fetch_one(pool)
        .await
}

async fn first_user_with_status(pool: &PgPool, status: &str) -> Result<Option<i64>, sqlx::Error> {
    sqlx::query_scalar("SELECT id FROM users WHERE status = $1 ORDER BY id LIMIT 1")
        .bind(status)
        .fetch_optional(pool)
        .await
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    #[tokio::test]
    async fn taken_ignores_case() {
        let db = TestDb::with_sql(SEED).await;
        assert!(username_taken(&db.pool, "alice").await.unwrap(), "alice занят");
        assert!(username_taken(&db.pool, "ALICE").await.unwrap(), "ники сравниваются без учёта регистра: ALICE это alice");
        assert!(!username_taken(&db.pool, "zed").await.unwrap(), "zed свободен");
    }

    #[tokio::test]
    async fn first_user_of_status() {
        let db = TestDb::with_sql(SEED).await;
        assert_eq!(first_user_with_status(&db.pool, "pending").await.unwrap(), Some(4), "среди ожидающих наименьший id у dave (4)");
        assert_eq!(first_user_with_status(&db.pool, "active").await.unwrap(), Some(1), "среди активных наименьший id у alice (1)");
        assert_eq!(first_user_with_status(&db.pool, "ghost").await.unwrap(), None, "такого статуса нет: None");
    }
}
