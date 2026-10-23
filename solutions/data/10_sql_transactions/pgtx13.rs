use sqlx::PgPool;

async fn edit_post(pool: &PgPool, id: i64, body: &str, hold_ms: i32) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT id FROM posts WHERE id = $1 FOR NO KEY UPDATE").bind(id).execute(&mut *tx).await?;
    sqlx::query("SELECT pg_sleep($1)").bind(hold_ms as f64 / 1000.0).execute(&mut *tx).await?;
    sqlx::query("UPDATE posts SET body = $2 WHERE id = $1").bind(id).bind(body).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SCHEMA: &str = "
        CREATE TABLE users (id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY);
        CREATE TABLE posts (id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY, body text NOT NULL);
        CREATE TABLE likes (post_id bigint NOT NULL REFERENCES posts (id), user_id bigint NOT NULL REFERENCES users (id), PRIMARY KEY (post_id, user_id));
        INSERT INTO users DEFAULT VALUES; INSERT INTO users DEFAULT VALUES;
        INSERT INTO posts (body) VALUES ('old');
    ";

    #[tokio::test]
    async fn likes_do_not_wait_for_an_edit() {
        let db = TestDb::with_sql(SCHEMA).await;
        let pool = db.pool.clone();
        let editor = tokio::spawn(async move { edit_post(&pool, 1, "new", 900).await });
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        let started = std::time::Instant::now();
        sqlx::query("INSERT INTO likes (post_id, user_id) VALUES (1, 1)").execute(&db.pool).await.expect("лайк");
        let waited = started.elapsed();
        editor.await.expect("задача упала").expect("правка");
        assert!(waited < std::time::Duration::from_millis(400), "лайк ждал {waited:?}: блокировка FOR UPDATE конфликтует с проверкой внешнего ключа, нужна более слабая");
        assert_eq!(db.table("SELECT body FROM posts").await.cell(0, 0), "new", "правка применилась");
    }

    #[tokio::test]
    async fn two_edits_still_queue_up() {
        let db = TestDb::with_sql(SCHEMA).await;
        let (p1, p2) = (db.pool.clone(), db.pool.clone());
        let first = tokio::spawn(async move { edit_post(&p1, 1, "first", 300).await });
        tokio::time::sleep(std::time::Duration::from_millis(80)).await;
        let started = std::time::Instant::now();
        edit_post(&p2, 1, "second", 10).await.expect("вторая правка");
        first.await.expect("задача упала").expect("первая правка");
        assert!(started.elapsed() >= std::time::Duration::from_millis(200), "вторая правка не дождалась первой: две правки одного поста обязаны идти по очереди");
        assert_eq!(db.table("SELECT body FROM posts").await.cell(0, 0), "second", "последней применилась вторая правка");
    }
}
