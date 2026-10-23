use sqlx::PgConnection;

async fn profile_line(db: &mut PgConnection, user_id: i64) -> Result<String, sqlx::Error> {
    let name: String = sqlx::query_scalar("SELECT username FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_one(&mut *db)
        .await?;
    let posts: i64 = sqlx::query_scalar("SELECT count(*) FROM posts WHERE author_id = $1")
        .bind(user_id)
        .fetch_one(&mut *db)
        .await?;
    Ok(format!("{name}: {posts}"))
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    #[tokio::test]
    #[allow(clippy::explicit_auto_deref)]
    async fn works_on_a_connection() {
        let db = TestDb::with_sql(SEED).await;
        let mut conn = db.pool.acquire().await.unwrap();
        let line = profile_line(&mut *conn, 1).await.expect("карточка anna");
        assert_eq!(line, "anna: 3", "у anna три поста");
    }

    #[tokio::test]
    #[allow(clippy::explicit_auto_deref)]
    async fn sees_uncommitted_rows_of_its_own_transaction() {
        let db = TestDb::with_sql(SEED).await;
        let mut tx = db.pool.begin().await.unwrap();
        sqlx::query("INSERT INTO posts (author_id, body) VALUES (1, 'новый')").execute(&mut *tx).await.unwrap();
        let line = profile_line(&mut *tx, 1).await.expect("карточка внутри транзакции");
        assert_eq!(line, "anna: 4", "внутри транзакции должен быть виден её собственный ещё не зафиксированный пост");
        tx.rollback().await.unwrap();
    }
}
