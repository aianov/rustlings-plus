use sqlx::{PgPool, Row};

async fn find_user_id(pool: &PgPool, username: &str) -> Result<Option<i64>, sqlx::Error> {
    let row = sqlx::query("SELECT id FROM users WHERE username = $1")
        .bind(username)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|r| r.get("id")))
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    #[tokio::test]
    async fn existing_user_is_found() {
        let db = TestDb::with_sql(SEED).await;
        assert_eq!(find_user_id(&db.pool, "carol").await.expect("запрос должен выполниться"), Some(3), "carol имеет id 3");
    }

    #[tokio::test]
    async fn missing_user_is_ok_none() {
        let db = TestDb::with_sql(SEED).await;
        let got = find_user_id(&db.pool, "nobody").await;
        assert!(matches!(got, Ok(None)), "для отсутствующего пользователя ждали Ok(None), а получили {got:?}");
    }

    #[tokio::test]
    async fn broken_database_is_still_an_error() {
        let db = TestDb::with_sql(SEED).await;
        db.pool.close().await;
        let got = find_user_id(&db.pool, "alice").await;
        assert!(got.is_err(), "когда база недоступна, это Err: превращать любую ошибку в None нельзя, иначе сбой выглядит как «пользователя нет»: {got:?}");
    }
}
