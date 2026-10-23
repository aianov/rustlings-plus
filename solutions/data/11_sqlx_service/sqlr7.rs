use sqlx::PgPool;
use sqlx::PgConnection;

async fn ensure_tag(conn: &mut PgConnection, name: &str) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar("INSERT INTO tags (name) VALUES ($1) ON CONFLICT (name) DO UPDATE SET name = EXCLUDED.name RETURNING id")
        .bind(name)
        .fetch_one(conn)
        .await
}

async fn publish(pool: &PgPool, author_id: i64, body: &str, tag: &str) -> Result<i64, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let post_id: i64 = sqlx::query_scalar("INSERT INTO posts (author_id, body, status) VALUES ($1, $2, 'published') RETURNING id")
        .bind(author_id)
        .bind(body)
        .fetch_one(&mut *tx)
        .await?;
    let tag_id = ensure_tag(&mut tx, tag).await?;
    sqlx::query("INSERT INTO post_tags (post_id, tag_id) VALUES ($1, $2)")
        .bind(post_id)
        .bind(tag_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(post_id)
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    async fn one_connection_pool(db: &TestDb) -> sqlx::PgPool {
        sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(std::time::Duration::from_secs(1))
            .connect_with(db.options())
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn publishes_on_a_single_connection_pool() {
        let db = TestDb::with_sql(SEED).await;
        let pool = one_connection_pool(&db).await;
        let res = tokio::time::timeout(std::time::Duration::from_secs(5), publish(&pool, 1, "про теги", "tips")).await;
        let id = res
            .expect("публикация зависла")
            .expect("публикация на пуле из одного соединения упала: транзакция держит единственное соединение, а вспомогательная функция просит ещё одно у пула");
        let t = db.table(&format!("SELECT t.name FROM post_tags pt JOIN tags t ON t.id = pt.tag_id WHERE pt.post_id = {id}")).await;
        assert_eq!(t.col("name"), vec!["tips"], "у нового поста должна быть привязка к тегу tips");
    }

    #[tokio::test]
    async fn existing_tag_is_reused() {
        let db = TestDb::with_sql(SEED).await;
        let pool = one_connection_pool(&db).await;
        publish(&pool, 1, "один", "rust").await.expect("первая публикация");
        publish(&pool, 2, "два", "rust").await.expect("вторая публикация");
        let t = db.table("SELECT count(*) FROM tags WHERE name = 'rust'").await;
        assert_eq!(t.cell(0, 0), "1", "тег rust уже есть, второй такой заводить нельзя");
        let t = db.table("SELECT count(*) FROM post_tags WHERE tag_id = 1").await;
        assert_eq!(t.cell(0, 0), "5", "к тегу rust было привязано 3 поста, добавилось ещё два");
    }
}
