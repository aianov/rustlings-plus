// Регистрация создаёт пользователя и сразу его первые посты. Если на втором посте что-то
// пошло не так (например, текст пустой), в базе не должно остаться ни поста, ни пользователя:
// иначе человек «зарегистрирован, но не может войти заново, потому что ник занят».
// Нужна транзакция: несколько команд, которые применяются все вместе или не применяются совсем.
// `register_with_posts` возвращает id пользователя.
// TODO: Реализуй функцию.

use sqlx::PgPool;

async fn register_with_posts(pool: &PgPool, username: &str, email: &str, posts: &[&str]) -> Result<i64, sqlx::Error> {
    todo!()
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    #[tokio::test]
    async fn everything_is_committed() {
        let db = TestDb::with_sql(SEED).await;
        let id = register_with_posts(&db.pool, "grace", "grace@example.com", &["один", "два"]).await.expect("регистрация должна пройти");
        assert_eq!(id, 7, "id нового пользователя");
        let t = db.table("SELECT body FROM posts WHERE author_id = 7 ORDER BY id").await;
        assert_eq!(t.column(0), ["один", "два"], "оба поста должны быть записаны и видны с другого соединения:\n{t}");
    }

    #[tokio::test]
    async fn failure_leaves_nothing_behind() {
        let db = TestDb::with_sql(SEED).await;
        let got = register_with_posts(&db.pool, "grace", "grace@example.com", &["нормальный пост", "   "]).await;
        assert!(got.is_err(), "пустой пост нарушает CHECK, функция должна вернуть ошибку");
        let users = db.table("SELECT count(*) FROM users").await;
        assert_eq!(users.cell(0, 0), "6", "пользователь не должен остаться: вся регистрация откатывается");
        let posts = db.table("SELECT count(*) FROM posts").await;
        assert_eq!(posts.cell(0, 0), "8", "и первый, нормальный пост тоже не должен остаться");
    }

    #[tokio::test]
    async fn after_rollback_the_name_is_free_again() {
        let db = TestDb::with_sql(SEED).await;
        assert!(register_with_posts(&db.pool, "grace", "grace@example.com", &[""]).await.is_err(), "пустой пост");
        let id = register_with_posts(&db.pool, "grace", "grace@example.com", &["теперь нормально"]).await.expect("после отката ник grace свободен");
        assert_eq!(id, 8, "счётчик id не откатывается: потраченный номер 7 пропущен, дальше идёт 8");
    }
}
