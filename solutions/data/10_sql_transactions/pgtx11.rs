use sqlx::PgPool;

fn constraint_sql() -> &'static str {
    "ALTER TABLE users ADD CONSTRAINT users_username_key UNIQUE (username)"
}

async fn register(pool: &PgPool, username: &str) -> Result<bool, sqlx::Error> {
    let res = sqlx::query("INSERT INTO users (username) VALUES ($1) ON CONFLICT (username) DO NOTHING")
        .bind(username)
        .execute(pool)
        .await?;
    Ok(res.rows_affected() == 1)
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
