use sqlx::PgPool;

#[derive(Debug, Clone, Copy, PartialEq, sqlx::Type)]
#[sqlx(type_name = "post_visibility", rename_all = "lowercase")]
enum Visibility {
    Public,
    Followers,
    Private,
}

async fn add_draft(pool: &PgPool, visibility: Visibility) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar("INSERT INTO drafts (visibility) VALUES ($1) RETURNING id")
        .bind(visibility)
        .fetch_one(pool)
        .await
}

async fn visibility_of(pool: &PgPool, id: i64) -> Result<Visibility, sqlx::Error> {
    sqlx::query_scalar("SELECT visibility FROM drafts WHERE id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
}

async fn count_with(pool: &PgPool, visibility: Visibility) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar("SELECT COUNT(*) FROM drafts WHERE visibility = $1")
        .bind(visibility)
        .fetch_one(pool)
        .await
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    #[tokio::test]
    async fn enum_roundtrip() {
        let db = TestDb::with_sql(
            "CREATE TYPE post_visibility AS ENUM ('public', 'followers', 'private');
             CREATE TABLE drafts (id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY, visibility post_visibility NOT NULL)",
        )
        .await;
        let a = add_draft(&db.pool, Visibility::Followers).await.expect("enum должен передаваться в запрос");
        let b = add_draft(&db.pool, Visibility::Private).await.unwrap();
        let _ = add_draft(&db.pool, Visibility::Followers).await.unwrap();
        let raw = db.table("SELECT visibility::text FROM drafts ORDER BY id").await;
        assert_eq!(raw.column(0), ["followers", "private", "followers"], "в базе метки строчными буквами: проверь переименование вариантов:\n{raw}");
        assert_eq!(visibility_of(&db.pool, a).await.expect("enum должен читаться из строки"), Visibility::Followers, "первый черновик для подписчиков");
        assert_eq!(visibility_of(&db.pool, b).await.unwrap(), Visibility::Private, "второй черновик приватный");
        assert_eq!(count_with(&db.pool, Visibility::Followers).await.unwrap(), 2, "черновиков для подписчиков два");
        assert_eq!(count_with(&db.pool, Visibility::Public).await.unwrap(), 0, "публичных нет");
    }
}
