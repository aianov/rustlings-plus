use sqlx::PgPool;

async fn set_status_for_all(pool: &PgPool, from: &str, to: &str) -> Result<u64, sqlx::Error> {
    let done = sqlx::query("UPDATE users SET status = $2 WHERE status = $1")
        .bind(from)
        .bind(to)
        .execute(pool)
        .await?;
    Ok(done.rows_affected())
}

async fn delete_own_post(pool: &PgPool, post_id: i64, author_id: i64) -> Result<bool, sqlx::Error> {
    let done = sqlx::query("DELETE FROM posts WHERE id = $1 AND author_id = $2")
        .bind(post_id)
        .bind(author_id)
        .execute(pool)
        .await?;
    Ok(done.rows_affected() > 0)
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    #[tokio::test]
    async fn counts_changed_users() {
        let db = TestDb::with_sql(SEED).await;
        assert_eq!(set_status_for_all(&db.pool, "pending", "active").await.unwrap(), 2, "ожидающих двое: dave и frank");
        assert_eq!(set_status_for_all(&db.pool, "pending", "active").await.unwrap(), 0, "второй раз менять некого, это 0, а не ошибка");
        let n = db.table("SELECT count(*) FROM users WHERE status = 'active'").await;
        assert_eq!(n.cell(0, 0), "5", "активных стало пятеро");
    }

    #[tokio::test]
    async fn author_deletes_own_post() {
        let db = TestDb::with_sql(SEED).await;
        assert!(delete_own_post(&db.pool, 1, 1).await.unwrap(), "пост 1 написал пользователь 1, удаление должно сработать");
        assert!(!delete_own_post(&db.pool, 1, 1).await.unwrap(), "второй раз поста уже нет: false");
        assert!(!delete_own_post(&db.pool, 9999, 1).await.unwrap(), "несуществующий пост: false, а не ошибка");
    }

    #[tokio::test]
    async fn foreign_post_stays() {
        let db = TestDb::with_sql(SEED).await;
        assert!(!delete_own_post(&db.pool, 3, 1).await.unwrap(), "пост 3 написал bob, alice его удалить не может");
        let n = db.table("SELECT count(*) FROM posts WHERE id = 3").await;
        assert_eq!(n.cell(0, 0), "1", "чужой пост должен остаться в таблице");
    }
}
