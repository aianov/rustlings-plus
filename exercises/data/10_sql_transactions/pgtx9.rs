// Автор может закрепить в профиле не больше трёх постов. `pin` считает уже закреплённые и, если
// меньше трёх, закрепляет ещё один (true), иначе возвращает false. Но когда автор нажимает кнопки
// почти одновременно, закреплёнными оказываются шесть-семь постов: каждая транзакция увидела, что
// закреплено меньше трёх. (Пауза имитирует работу между проверкой и записью.) Восемь одновременных
// вызовов должны закрепить ровно три поста.
// TODO: Найди, чего не хватает проверке, и исправь.

use sqlx::PgPool;

async fn pin(pool: &PgPool, author_id: i64, post_id: i64) -> Result<bool, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let pinned: i64 = sqlx::query_scalar("SELECT count(*) FROM posts WHERE author_id = $1 AND pinned").bind(author_id).fetch_one(&mut *tx).await?;
    if pinned >= 3 {
        return Ok(false);
    }
    sqlx::query("SELECT pg_sleep(0.05)").execute(&mut *tx).await?;
    sqlx::query("UPDATE posts SET pinned = true WHERE id = $1").bind(post_id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(true)
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    #[tokio::test]
    async fn never_more_than_three_pinned() {
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
            if h.await.expect("задача упала").expect("pin вернул ошибку") {
                ok += 1;
            }
        }
        let pinned = db.table("SELECT id FROM posts WHERE pinned").await.len();
        assert_eq!(pinned, 3, "закреплено {pinned} постов, а правило «не больше трёх»: проверка по двум транзакциям одновременно устарела (write skew)");
        assert_eq!(ok, 3, "true должны вернуть ровно три вызова");
    }
}
