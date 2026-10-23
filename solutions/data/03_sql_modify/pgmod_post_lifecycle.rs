fn create_post() -> &'static str {
    "INSERT INTO posts (author_id, body) VALUES ($1, $2) RETURNING id"
}

fn edit_post() -> &'static str {
    "UPDATE posts SET body = $3, updated_at = now() WHERE id = $1 AND author_id = $2 RETURNING id"
}

fn like_post() -> &'static str {
    "INSERT INTO post_likes (post_id, user_id) VALUES ($1, $2) ON CONFLICT DO NOTHING RETURNING post_id"
}

fn bump_likes() -> &'static str {
    "UPDATE posts SET likes_count = likes_count + 1 WHERE id = $1"
}

fn delete_post() -> &'static str {
    "DELETE FROM posts WHERE id = $1 AND author_id = $2 RETURNING id"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");


    #[tokio::test]
    async fn create_returns_id() {
        let db = TestDb::with_sql(SEED).await;
        let id: i64 = sqlx::query_scalar(create_post()).bind(2i64).bind("fresh").fetch_one(&db.pool).await.unwrap_or_else(|e| panic!("create_post: {e}"));
        let t = db.table(&format!("SELECT author_id, body, updated_at IS NULL FROM posts WHERE id = {id}")).await;
        assert_eq!(t.rows, [["2", "fresh", "true"]], "пост создан автором 2, updated_at пока пуст:\n{t}");
    }

    #[tokio::test]
    async fn only_author_can_edit() {
        let db = TestDb::with_sql(SEED).await;
        let foreign: Option<i64> = sqlx::query_scalar(edit_post()).bind(1i64).bind(2i64).bind("hacked").fetch_optional(&db.pool).await.unwrap_or_else(|e| panic!("edit_post: {e}"));
        assert_eq!(foreign, None, "bob (2) не автор поста 1: правка не должна пройти");
        let t = db.table("SELECT body <> 'hacked', updated_at IS NULL FROM posts WHERE id = 1").await;
        assert_eq!(t.rows, [["true", "true"]], "чужая попытка не должна оставить следов: ни текста, ни updated_at:\n{t}");
        let own: Option<i64> = sqlx::query_scalar(edit_post()).bind(1i64).bind(1i64).bind("edited").fetch_optional(&db.pool).await.unwrap_or_else(|e| panic!("edit_post: {e}"));
        assert_eq!(own, Some(1), "автор правит свой пост и получает его id");
        let t = db.table("SELECT body, updated_at IS NOT NULL FROM posts WHERE id = 1").await;
        assert_eq!(t.rows, [["edited", "true"]], "текст изменён, updated_at проставлен (а не от чужой попытки):\n{t}");
    }

    #[tokio::test]
    async fn like_is_idempotent_and_reports_novelty() {
        let db = TestDb::with_sql(SEED).await;
        let first: Option<i64> = sqlx::query_scalar(like_post()).bind(3i64).bind(1i64).fetch_optional(&db.pool).await.unwrap_or_else(|e| panic!("like_post: {e}"));
        let again: Option<i64> = sqlx::query_scalar(like_post()).bind(3i64).bind(1i64).fetch_optional(&db.pool).await.unwrap_or_else(|e| panic!("повторный like_post не должен падать: {e}"));
        assert_eq!(first, Some(3), "новый лайк возвращает id поста");
        assert_eq!(again, None, "повторный лайк ничего не возвращает: приложение не должно увеличивать счётчик второй раз");
        assert_eq!(db.table("SELECT * FROM post_likes").await.len(), 1, "лайк в таблице один");
    }

    #[tokio::test]
    async fn counter_grows_atomically() {
        let db = TestDb::with_sql(SEED).await;
        let mut handles = Vec::new();
        for _ in 0..10 {
            let pool = db.pool.clone();
            handles.push(tokio::spawn(async move { sqlx::query(bump_likes()).bind(1i64).execute(&pool).await }));
        }
        for h in handles {
            h.await.expect("задача упала").unwrap_or_else(|e| panic!("bump_likes: {e}"));
        }
        assert_eq!(db.table("SELECT likes_count FROM posts WHERE id = 1").await.cell(0, 0), "13", "было 3, добавили 10");
    }

    #[tokio::test]
    async fn only_author_can_delete() {
        let db = TestDb::with_sql(SEED).await;
        let foreign: Option<i64> = sqlx::query_scalar(delete_post()).bind(1i64).bind(2i64).fetch_optional(&db.pool).await.unwrap_or_else(|e| panic!("delete_post: {e}"));
        assert_eq!(foreign, None, "чужой пост удалять нельзя");
        assert_eq!(db.table("SELECT id FROM posts WHERE id = 1").await.len(), 1, "пост 1 должен остаться");
        let own: Option<i64> = sqlx::query_scalar(delete_post()).bind(1i64).bind(1i64).fetch_optional(&db.pool).await.unwrap_or_else(|e| panic!("delete_post: {e}"));
        assert_eq!(own, Some(1), "автор удаляет свой пост и получает его id");
        assert_eq!(db.table("SELECT id FROM posts WHERE id = 1").await.len(), 0, "пост 1 должен исчезнуть");
    }
}
