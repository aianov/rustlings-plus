// Публикация поста с тегом это два INSERT: сам пост и связь поста с тегом. Если второй упадёт
// (например, тега с таким id нет), в базе остаётся пост без тега, который никто не просил.
// Функция `publish` должна создать пост и привязать тег так, чтобы либо были оба, либо ни одного.
// Вернуть нужно id нового поста.
// TODO: Допиши функцию.

use sqlx::PgPool;

async fn publish(pool: &PgPool, author_id: i64, body: &str, tag_id: i64) -> Result<i64, sqlx::Error> {
    let id: i64 = sqlx::query_scalar("INSERT INTO posts (author_id, body) VALUES ($1, $2) RETURNING id")
        .bind(author_id)
        .bind(body)
        .fetch_one(pool)
        .await?;
    sqlx::query("INSERT INTO post_tags (post_id, tag_id) VALUES ($1, $2)")
        .bind(id)
        .bind(tag_id)
        .execute(pool)
        .await?;
    Ok(id)
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SCHEMA: &str = "
        CREATE TABLE users (id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY);
        CREATE TABLE tags (id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY);
        CREATE TABLE posts (id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY, author_id bigint NOT NULL REFERENCES users (id), body text NOT NULL);
        CREATE TABLE post_tags (post_id bigint NOT NULL REFERENCES posts (id), tag_id bigint NOT NULL REFERENCES tags (id), PRIMARY KEY (post_id, tag_id));
        INSERT INTO users DEFAULT VALUES; INSERT INTO tags DEFAULT VALUES;
    ";

    #[tokio::test]
    async fn both_rows_are_created() {
        let db = TestDb::with_sql(SCHEMA).await;
        let id = publish(&db.pool, 1, "hello", 1).await.expect("публикация с существующим тегом должна пройти");
        assert_eq!(db.table(&format!("SELECT tag_id FROM post_tags WHERE post_id = {id}")).await.len(), 1, "связь с тегом должна быть создана");
    }

    #[tokio::test]
    async fn failure_leaves_nothing_behind() {
        let db = TestDb::with_sql(SCHEMA).await;
        let res = publish(&db.pool, 1, "orphan", 99).await;
        assert!(res.is_err(), "тега 99 нет, публикация должна закончиться ошибкой");
        let t = db.table("SELECT id, body FROM posts").await;
        assert!(t.is_empty(), "пост без тега не должен остаться, если привязка тега не удалась:\n{t}");
    }
}
