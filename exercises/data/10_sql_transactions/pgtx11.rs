// Регистрация проверяет, свободен ли ник, и потом вставляет пользователя. При двух одновременных
// регистрациях одного ника оба успевают пройти проверку, и в таблице два одинаковых ника.
// (Пауза в коде имитирует работу между проверкой и вставкой.) Нужны две вещи: SQL-команда
// `constraint_sql`, которая сделает дубль невозможным (её тест выполнит перед регистрациями), и
// `register`, возвращающая true для первого и false для остальных, без ошибок наружу.
// TODO: Найди причину и исправь обе функции.

use sqlx::PgPool;

fn constraint_sql() -> &'static str {
    "SELECT 1"
}

async fn register(pool: &PgPool, username: &str) -> Result<bool, sqlx::Error> {
    let taken: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM users WHERE username = $1)").bind(username).fetch_one(pool).await?;
    if taken {
        return Ok(false);
    }
    sqlx::query("SELECT pg_sleep(0.05)").execute(pool).await?;
    sqlx::query("INSERT INTO users (username) VALUES ($1)").bind(username).execute(pool).await?;
    Ok(true)
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    #[tokio::test]
    async fn only_one_registration_wins() {
        let db = TestDb::with_sql("CREATE TABLE users (id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY, username text NOT NULL)").await;
        db.run(constraint_sql()).await;
        let mut handles = Vec::new();
        for _ in 0..12 {
            let pool = db.pool.clone();
            handles.push(tokio::spawn(async move { register(&pool, "alice").await }));
        }
        let mut wins = 0;
        for h in handles {
            if h.await.expect("задача упала").expect("register не должна возвращать ошибку наружу: проигравший получает false") {
                wins += 1;
            }
        }
        assert_eq!(wins, 1, "победителей {wins}: проверка «ника нет» и вставка разделены, между ними успевают другие");
        assert_eq!(db.table("SELECT id FROM users").await.len(), 1, "в таблице должен остаться один alice");
    }

    #[tokio::test]
    async fn different_names_all_register() {
        let db = TestDb::with_sql("CREATE TABLE users (id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY, username text NOT NULL)").await;
        db.run(constraint_sql()).await;
        for name in ["a", "b", "c"] {
            assert!(register(&db.pool, name).await.expect("вставка"), "ник {name} свободен");
        }
        assert!(!register(&db.pool, "b").await.expect("вставка"), "ник b уже занят");
    }
}
