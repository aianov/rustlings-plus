fn publish() -> &'static str {
    "WITH new_post AS (
         INSERT INTO posts (author_id, body) VALUES ($1, $2) RETURNING id
     ),
     attached AS (
         INSERT INTO post_tags (post_id, tag_id)
         SELECT new_post.id, t.id FROM new_post, tags t WHERE t.name = ANY($3)
     )
     SELECT id FROM new_post"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    async fn publish_post(db: &TestDb, tags: &[&str]) -> i64 {
        let tags: Vec<String> = tags.iter().map(|t| t.to_string()).collect();
        sqlx::query_scalar::<_, i64>(publish())
            .bind(2_i64)
            .bind("brand new")
            .bind(tags)
            .fetch_one(&db.pool)
            .await
            .unwrap_or_else(|e| panic!("publish: {e}"))
    }

    #[tokio::test]
    async fn post_and_tags_created_together() {
        let db = TestDb::with_sql(SEED).await;
        let id = publish_post(&db, &["rust", "sql", "nope"]).await;
        let t = db.table(&format!("SELECT author_id, body FROM posts WHERE id = {id}")).await;
        assert_eq!(t.rows, [["2", "brand new"]], "вернувшийся id должен быть id нового поста:\n{t}");
        let t = db.table(&format!("SELECT t.name FROM post_tags pt JOIN tags t ON t.id = pt.tag_id WHERE pt.post_id = {id} ORDER BY t.name")).await;
        assert_eq!(t.rows, [["rust"], ["sql"]], "к посту привязываются известные теги rust и sql, nope игнорируется:\n{t}");
        assert_eq!(db.table("SELECT id FROM tags").await.len(), 3, "новых тегов запрос создавать не должен");
    }

    #[tokio::test]
    async fn tags_are_attached_even_though_the_result_does_not_read_them() {
        let db = TestDb::with_sql(SEED).await;
        let id = publish_post(&db, &["async"]).await;
        let n = db.table(&format!("SELECT 1 FROM post_tags WHERE post_id = {id}")).await.len();
        assert_eq!(n, 1, "результат запроса не читает привязку тегов, но она всё равно должна выполниться");
        assert_eq!(db.table("SELECT id FROM posts").await.len(), 11, "постов должно стать одиннадцать");
    }

    #[tokio::test]
    async fn no_tags_still_publishes() {
        let db = TestDb::with_sql(SEED).await;
        let id = publish_post(&db, &[]).await;
        assert!(id > 10, "пост без тегов тоже должен опубликоваться, id {id}");
        assert_eq!(db.table("SELECT 1 FROM post_tags").await.len(), 3, "привязок тегов не прибавилось");
    }
}
