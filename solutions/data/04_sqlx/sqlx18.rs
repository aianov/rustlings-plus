use sqlx::{PgConnection, PgPool, Row};

async fn create_post_with_tags(pool: &PgPool, author_id: i64, body: &str, tags: &[&str]) -> Result<i64, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let post_id: i64 = sqlx::query("INSERT INTO posts (author_id, body) VALUES ($1, $2) RETURNING id")
        .bind(author_id)
        .bind(body)
        .fetch_one(&mut *tx)
        .await?
        .get("id");
    for tag in tags {
        attach_tag(&mut tx, post_id, tag).await?;
    }
    tx.commit().await?;
    Ok(post_id)
}

async fn attach_tag(conn: &mut PgConnection, post_id: i64, name: &str) -> Result<(), sqlx::Error> {
    let tag_id: i64 = sqlx::query_scalar(
        "INSERT INTO tags (name) VALUES ($1) ON CONFLICT (name) DO UPDATE SET name = EXCLUDED.name RETURNING id",
    )
    .bind(name)
    .fetch_one(&mut *conn)
    .await?;
    sqlx::query("INSERT INTO post_tags (post_id, tag_id) VALUES ($1, $2)")
        .bind(post_id)
        .bind(tag_id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    #[tokio::test]
    async fn post_with_tags_is_stored() {
        let db = TestDb::with_sql(SEED).await;
        let id = create_post_with_tags(&db.pool, 4, "Привет, мир", &["rust", "fresh"]).await.expect("публикация с тегами должна пройти");
        let t = db.table(&format!("SELECT t.name FROM post_tags pt JOIN tags t ON t.id = pt.tag_id WHERE pt.post_id = {id} ORDER BY t.name")).await;
        assert_eq!(t.column(0), ["fresh", "rust"], "у поста два тега: существующий и новый:\n{t}");
        let tags = db.table("SELECT count(*) FROM tags").await;
        assert_eq!(tags.cell(0, 0), "4", "тег rust уже был, fresh создан: всего четыре");
    }

    #[tokio::test]
    async fn rejected_tag_rolls_everything_back() {
        let db = TestDb::with_sql(SEED).await;
        let too_long = "x".repeat(40);
        let got = create_post_with_tags(&db.pool, 4, "Не должен сохраниться", &["fresh", &too_long]).await;
        assert!(got.is_err(), "слишком длинный тег нарушает CHECK, ждали ошибку");
        let posts = db.table("SELECT count(*) FROM posts").await;
        assert_eq!(posts.cell(0, 0), "8", "пост не должен остаться без тегов");
        let tags = db.table("SELECT count(*) FROM tags").await;
        assert_eq!(tags.cell(0, 0), "3", "тег fresh, созданный по пути, тоже должен откатиться");
        let links = db.table("SELECT count(*) FROM post_tags").await;
        assert_eq!(links.cell(0, 0), "4", "новых связей быть не должно");
    }

    #[tokio::test]
    async fn two_posts_share_a_tag() {
        let db = TestDb::with_sql(SEED).await;
        create_post_with_tags(&db.pool, 4, "первый", &["shared"]).await.unwrap();
        create_post_with_tags(&db.pool, 4, "второй", &["shared"]).await.expect("повторное использование тега не ошибка");
        let tags = db.table("SELECT count(*) FROM tags WHERE name = 'shared'").await;
        assert_eq!(tags.cell(0, 0), "1", "тег shared существует в одном экземпляре");
    }
}
