use sqlx::PgPool;

#[derive(Debug, sqlx::FromRow)]
struct User {
    id: i64,
    username: String,
    email: String,
    status: String,
}

async fn user_by_id(pool: &PgPool, id: i64) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as::<_, User>("SELECT id, username, email, status FROM users WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await
}

async fn users_page(pool: &PgPool, limit: i64, offset: i64) -> Result<Vec<User>, sqlx::Error> {
    sqlx::query_as::<_, User>(
        "SELECT id, username, email, status FROM users ORDER BY created_at DESC, id LIMIT $1 OFFSET $2",
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    #[tokio::test]
    async fn loads_one_user() {
        let db = TestDb::with_sql(SEED).await;
        let u = user_by_id(&db.pool, 3).await.expect("запрос должен выполниться").expect("carol должна найтись");
        assert_eq!((u.id, u.username.as_str(), u.email.as_str(), u.status.as_str()), (3, "carol", "carol@mail.org", "banned"), "поля пользователя 3: {u:?}");
        assert!(user_by_id(&db.pool, 77).await.unwrap().is_none(), "пользователя 77 нет");
    }

    #[tokio::test]
    async fn pages_are_newest_first() {
        let db = TestDb::with_sql(SEED).await;
        let first = users_page(&db.pool, 3, 0).await.unwrap();
        let names: Vec<&str> = first.iter().map(|u| u.username.as_str()).collect();
        assert_eq!(names, ["dave", "alice", "frank"], "сначала самые новые, у alice и frank дата одна, там порядок по id");
        let second = users_page(&db.pool, 3, 3).await.unwrap();
        let names: Vec<&str> = second.iter().map(|u| u.username.as_str()).collect();
        assert_eq!(names, ["carol", "bob", "eve"], "вторая страница продолжает первую");
        assert!(users_page(&db.pool, 3, 6).await.unwrap().is_empty(), "после последней страницы пусто");
    }
}
