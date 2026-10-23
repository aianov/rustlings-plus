// Регистрация ломается о два разных правила: ник уникален и почта уникальна. Человеку надо
// сказать, что именно занято, «ник» или «почта», а код ошибки 23505 в обоих случаях один и тот
// же. Различить их можно по имени нарушенного ограничения (в схеме курса это
// users_username_uniq и users_email_uniq). Другие ошибки базы, например слишком короткий ник,
// заворачивай в Db.
// TODO: Реализуй register и разбор ошибки.

use sqlx::PgPool;

#[derive(Debug)]
enum RegisterError {
    UsernameTaken,
    EmailTaken,
    Db(sqlx::Error),
}

async fn register(pool: &PgPool, username: &str, email: &str) -> Result<i64, RegisterError> {
    todo!()
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    #[tokio::test]
    async fn free_name_and_email() {
        let db = TestDb::with_sql(SEED).await;
        assert_eq!(register(&db.pool, "grace", "grace@example.com").await.expect("свободные данные"), 7, "седьмой пользователь получает id 7");
    }

    #[tokio::test]
    async fn username_and_email_are_told_apart() {
        let db = TestDb::with_sql(SEED).await;
        let got = register(&db.pool, "alice", "new@example.com").await;
        assert!(matches!(got, Err(RegisterError::UsernameTaken)), "ник alice занят, почта свободна: {got:?}");
        let got = register(&db.pool, "newbie", "bob@example.com").await;
        assert!(matches!(got, Err(RegisterError::EmailTaken)), "почта bob занята, ник свободен: {got:?}");
    }

    #[tokio::test]
    async fn both_taken_is_one_of_the_two() {
        let db = TestDb::with_sql(SEED).await;
        let got = register(&db.pool, "alice", "alice@example.com").await;
        assert!(matches!(got, Err(RegisterError::UsernameTaken) | Err(RegisterError::EmailTaken)), "заняты оба, ждали любое из двух «занято»: {got:?}");
    }

    #[tokio::test]
    async fn other_violations_stay_db_errors() {
        let db = TestDb::with_sql(SEED).await;
        let got = register(&db.pool, "ab", "short@example.com").await;
        assert!(matches!(got, Err(RegisterError::Db(_))), "слишком короткий ник нарушает CHECK, это не «занято»: {got:?}");
        db.pool.close().await;
        match register(&db.pool, "zed", "zed@example.com").await {
            Err(RegisterError::Db(e)) => assert!(matches!(e, sqlx::Error::PoolClosed), "закрытый пул должен дать PoolClosed внутри Db, а вышло {e:?}"),
            other => panic!("недоступная база это Db, а вышло {other:?}"),
        }
    }
}
