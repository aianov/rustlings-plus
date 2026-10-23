// Лента показывает число лайков под постом. Считать count(*) по likes на каждый показ дорого,
// поэтому счётчик хранится в posts.likes_count. Но счётчик и сами лайки должны не расходиться.
// Таблицы users, posts (с likes_count) и likes (пара user_id, post_id, первичный ключ) уже есть.
//  - `like_post` ($1 пользователь, $2 пост): ставит лайк и возвращает новое число лайков поста.
//    Повторный лайк того же пользователя ничего не меняет.
//  - `unlike_post` ($1, $2): снимает лайк и возвращает число лайков. Если лайка не было, счётчик
//    не уменьшается.
// Каждая функция это один оператор SQL, без гонок при параллельных вызовах. Лайк несуществующего
// поста должен закончиться ошибкой.
// TODO: Напиши оба запроса.

fn like_post() -> &'static str {
    "UPDATE posts SET likes_count = likes_count + 1 WHERE id = $2 AND $1 > 0 RETURNING likes_count"
}

fn unlike_post() -> &'static str {
    "UPDATE posts SET likes_count = likes_count - 1 WHERE id = $2 AND $1 > 0 RETURNING likes_count"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;


    async fn prepared() -> TestDb {
        let db = TestDb::new().await;
        db.run(
            "CREATE TABLE users (id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY);
             CREATE TABLE posts (id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY, likes_count integer NOT NULL DEFAULT 0 CHECK (likes_count >= 0));
             CREATE TABLE likes (user_id bigint NOT NULL REFERENCES users (id), post_id bigint NOT NULL REFERENCES posts (id), PRIMARY KEY (user_id, post_id));
             INSERT INTO users (id) OVERRIDING SYSTEM VALUE SELECT g FROM generate_series(1, 20) g;
             INSERT INTO posts DEFAULT VALUES; INSERT INTO posts DEFAULT VALUES",
        )
        .await;
        db
    }

    async fn run(pool: &sqlx::PgPool, sql: &str, user: i64, post: i64) -> i32 {
        sqlx::query_scalar::<_, i32>(sql)
            .bind(user)
            .bind(post)
            .fetch_one(pool)
            .await
            .unwrap_or_else(|e| panic!("запрос не выполнился: {e}"))
    }

    async fn check_invariant(db: &TestDb) {
        let t = db.table("SELECT p.id, p.likes_count, (SELECT count(*) FROM likes l WHERE l.post_id = p.id) AS actual FROM posts p WHERE p.likes_count <> (SELECT count(*) FROM likes l WHERE l.post_id = p.id)").await;
        assert!(t.is_empty(), "счётчик должен совпадать с настоящим числом лайков:\n{t}");
    }

    #[tokio::test]
    async fn counter_follows_likes() {
        let db = prepared().await;
        assert_eq!(run(&db.pool, like_post(), 1, 1).await, 1, "первый лайк даёт 1");
        assert_eq!(run(&db.pool, like_post(), 2, 1).await, 2, "второй пользователь даёт 2");
        assert_eq!(run(&db.pool, like_post(), 1, 1).await, 2, "повторный лайк того же пользователя не меняет счётчик");
        assert_eq!(run(&db.pool, unlike_post(), 2, 1).await, 1, "снятие лайка уменьшает счётчик");
        assert_eq!(run(&db.pool, unlike_post(), 2, 1).await, 1, "повторное снятие не должно уйти в минус или уменьшить счётчик ещё раз");
        assert_eq!(run(&db.pool, like_post(), 3, 2).await, 1, "посты считаются независимо");
        check_invariant(&db).await;
    }

    #[tokio::test]
    async fn missing_post_changes_nothing() {
        let db = prepared().await;
        let err = sqlx::query_scalar::<_, i32>(like_post()).bind(1_i64).bind(99_i64).fetch_one(&db.pool).await;
        assert!(err.is_err(), "лайк несуществующего поста должен закончиться ошибкой, а не тихо пройти");
        assert_eq!(db.table("SELECT * FROM likes").await.len(), 0, "в likes ничего не должно появиться");
    }

    #[tokio::test]
    async fn parallel_likes_from_different_users() {
        let db = prepared().await;
        let mut handles = Vec::new();
        for user in 1..=12_i64 {
            let pool = db.pool.clone();
            handles.push(tokio::spawn(async move { run(&pool, like_post(), user, 1).await }));
        }
        for h in handles {
            h.await.expect("задача упала");
        }
        assert_eq!(db.table("SELECT likes_count FROM posts WHERE id = 1").await.cell(0, 0), "12", "двенадцать разных лайков параллельно дают 12: часть обновлений счётчика потерялась");
        check_invariant(&db).await;
    }

    #[tokio::test]
    async fn parallel_double_clicks_count_once() {
        let db = prepared().await;
        let mut handles = Vec::new();
        for _ in 0..12 {
            let pool = db.pool.clone();
            handles.push(tokio::spawn(async move { run(&pool, like_post(), 7, 1).await }));
        }
        for h in handles {
            h.await.expect("задача упала");
        }
        assert_eq!(db.table("SELECT likes_count FROM posts WHERE id = 1").await.cell(0, 0), "1", "двенадцать одинаковых лайков одного человека дают 1");
        check_invariant(&db).await;
    }
}
