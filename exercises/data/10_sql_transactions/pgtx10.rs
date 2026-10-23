// Снова «не больше трёх закреплённых постов», но на этот раз без явных блокировок: слова FOR UPDATE
// в коде быть не должно. Правило должны защитить сами гарантии транзакций (уровень SERIALIZABLE).
// База сама заметит, что транзакции зависят друг от друга, и одной из них вернёт ошибку 40001; такую
// транзакцию надо выполнить заново. Восемь одновременных вызовов должны все вернуть Ok и закрепить
// ровно три поста.
// TODO: Допиши `pin`.

use sqlx::PgPool;

async fn attempt(pool: &PgPool, author_id: i64, post_id: i64) -> Result<bool, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let pinned: i64 = sqlx::query_scalar("SELECT count(*) FROM posts WHERE author_id = $1 AND pinned").bind(author_id).fetch_one(&mut *tx).await?;
    if pinned >= 3 {
        return Ok(false);
    }
    sqlx::query("SELECT pg_sleep(0.03)").execute(&mut *tx).await?;
    sqlx::query("UPDATE posts SET pinned = true WHERE id = $1").bind(post_id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(true)
}

async fn pin(pool: &PgPool, author_id: i64, post_id: i64) -> Result<bool, sqlx::Error> {
    attempt(pool, author_id, post_id).await
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    #[test]
    fn no_explicit_locks_in_code() {
        let src = include_str!("pgtx10.rs");
        let code = src.split("#[cfg(test)]").next().unwrap_or("");
        assert!(!code.to_uppercase().contains("FOR UPDATE"), "явные блокировки запрещены условием задачи: защиту должен дать уровень SERIALIZABLE");
        assert!(code.to_uppercase().contains("SERIALIZABLE"), "в коде должен быть уровень SERIALIZABLE");
    }

    #[tokio::test]
    async fn serializable_keeps_the_rule() {
        let db = TestDb::with_sql("
            CREATE TABLE users (id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY);
            CREATE TABLE posts (id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY, author_id bigint NOT NULL REFERENCES users (id), pinned boolean NOT NULL DEFAULT false);
            INSERT INTO users DEFAULT VALUES;
            INSERT INTO posts (author_id) SELECT 1 FROM generate_series(1, 8);
        ").await;
        let mut handles = Vec::new();
        for post in 1..=8_i64 {
            let pool = db.pool.clone();
            handles.push(tokio::spawn(async move { pin(&pool, 1, post).await }));
        }
        let mut ok = 0;
        for h in handles {
            match h.await.expect("задача упала") {
                Ok(true) => ok += 1,
                Ok(false) => {}
                Err(e) => panic!("pin вернул ошибку {e}: сбой сериализации (40001) нужно ловить и повторять транзакцию"),
            }
        }
        let pinned = db.table("SELECT id FROM posts WHERE pinned").await.len();
        assert_eq!(pinned, 3, "закреплено {pinned}, а должно быть ровно три");
        assert_eq!(ok, 3, "true должны вернуть ровно три вызова");
    }
}
