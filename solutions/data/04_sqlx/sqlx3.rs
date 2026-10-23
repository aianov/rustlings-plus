use sqlx::{PgPool, Row};

async fn all_usernames(pool: &PgPool) -> Result<Vec<String>, sqlx::Error> {
    let rows = sqlx::query("SELECT username FROM users ORDER BY id").fetch_all(pool).await?;
    Ok(rows.iter().map(|r| r.get::<String, _>("username")).collect())
}

async fn find_email(pool: &PgPool, username: &str) -> Result<Option<String>, sqlx::Error> {
    let row = sqlx::query("SELECT email FROM users WHERE username = $1")
        .bind(username)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|r| r.get::<String, _>("email")))
}

async fn email_of(pool: &PgPool, id: i64) -> Result<String, sqlx::Error> {
    let row = sqlx::query("SELECT email FROM users WHERE id = $1").bind(id).fetch_one(pool).await?;
    Ok(row.get::<String, _>("email"))
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    #[tokio::test]
    async fn lists_everyone_in_id_order() {
        let db = TestDb::with_sql(SEED).await;
        let got = all_usernames(&db.pool).await.expect("запрос должен выполниться");
        assert_eq!(got, ["alice", "bob", "carol", "dave", "eve", "frank"], "ники в порядке id");
        db.run("DELETE FROM sessions; DELETE FROM post_tags; DELETE FROM posts; DELETE FROM users").await;
        assert!(all_usernames(&db.pool).await.expect("пустая таблица это не ошибка").is_empty(), "на пустой таблице список пуст, а не ошибка");
    }

    #[tokio::test]
    async fn missing_user_is_none() {
        let db = TestDb::with_sql(SEED).await;
        assert_eq!(find_email(&db.pool, "alice").await.unwrap().as_deref(), Some("alice@example.com"), "почта alice");
        assert_eq!(find_email(&db.pool, "nobody").await.expect("отсутствие строки не ошибка"), None, "для неизвестного ника должно быть None");
    }

    #[tokio::test]
    async fn must_exist_is_row_not_found() {
        let db = TestDb::with_sql(SEED).await;
        assert_eq!(email_of(&db.pool, 2).await.unwrap(), "bob@example.com", "почта пользователя 2");
        let err = email_of(&db.pool, 99).await.expect_err("пользователя 99 нет: ждали ошибку");
        assert!(matches!(err, sqlx::Error::RowNotFound), "ждали RowNotFound, получили {err:?}");
    }
}
