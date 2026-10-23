// Жизненный цикл поста в пяти запросах с параметрами. Тест ведёт себя как приложение.
// 1. create_post: $1 автор, $2 текст. Создаёт пост и возвращает его id.
// 2. edit_post: $1 id поста, $2 автор, $3 новый текст. Меняет текст и ставит updated_at = now(),
//    но только если пост принадлежит этому автору. Возвращает id изменённого поста;
//    чужой или несуществующий пост не меняется и ничего не возвращает.
// 3. like_post: $1 id поста, $2 id пользователя. Повторный лайк того же пользователя не ошибка и
//    не создаёт дубля. Возвращает post_id только если лайк новый (приложение по этому решает,
//    увеличивать ли счётчик).
// 4. bump_likes: $1 id поста. Увеличивает likes_count на единицу атомарно.
// 5. delete_post: $1 id поста, $2 автор. Удаляет пост только автору и возвращает его id.
//    Чужой пост не удаляется.
// TODO: Напиши запросы.

fn create_post() -> &'static str {
    todo!()
}

fn edit_post() -> &'static str {
    todo!()
}

fn like_post() -> &'static str {
    todo!()
}

fn bump_likes() -> &'static str {
    todo!()
}

fn delete_post() -> &'static str {
    todo!()
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
