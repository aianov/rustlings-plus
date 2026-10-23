// Массовое проставление тегов. `tag_post(post_id, tag_ids)` отмечает пост как размеченный
// (posts.tagged = true) и привязывает перечисленные теги. Некоторых тегов может не существовать:
// такие привязки пропускаются (внешний ключ их отвергнет), остальные должны сохраниться. Всё идёт
// одной транзакцией, а вернуть нужно число привязанных тегов. Если поста нет, это ошибка.
// TODO: Допиши функцию.

use sqlx::PgPool;

async fn tag_post(pool: &PgPool, post_id: i64, tag_ids: &[i64]) -> Result<usize, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let marked = sqlx::query("UPDATE posts SET tagged = true WHERE id = $1").bind(post_id).execute(&mut *tx).await?;
    if marked.rows_affected() == 0 {
        return Err(sqlx::Error::RowNotFound);
    }
    let mut linked = 0;
    for tag in tag_ids {
        if sqlx::query("INSERT INTO post_tags (post_id, tag_id) VALUES ($1, $2)").bind(post_id).bind(tag).execute(&mut *tx).await.is_ok() {
            linked += 1;
        }
    }
    tx.commit().await?;
    Ok(linked)
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SCHEMA: &str = "
        CREATE TABLE tags (id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY);
        CREATE TABLE posts (id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY, tagged boolean NOT NULL DEFAULT false);
        CREATE TABLE post_tags (post_id bigint NOT NULL REFERENCES posts (id), tag_id bigint NOT NULL REFERENCES tags (id), PRIMARY KEY (post_id, tag_id));
        INSERT INTO tags SELECT FROM generate_series(1, 3);
        INSERT INTO posts DEFAULT VALUES;
    ";

    #[tokio::test]
    async fn bad_tags_are_skipped_good_ones_stay() {
        let db = TestDb::with_sql(SCHEMA).await;
        let n = tag_post(&db.pool, 1, &[1, 99, 2, 98, 3]).await.expect("неизвестные теги не должны ронять функцию");
        assert_eq!(n, 3, "привязаться должны теги 1, 2 и 3");
        assert_eq!(db.table("SELECT * FROM post_tags").await.len(), 3, "в базе три связи: откат одной неудачной вставки не должен уносить удачные");
        assert_eq!(db.table("SELECT tagged::text FROM posts WHERE id = 1").await.cell(0, 0), "true", "пост помечен как размеченный в той же транзакции");
    }

    #[tokio::test]
    async fn missing_post_is_an_error() {
        let db = TestDb::with_sql(SCHEMA).await;
        assert!(tag_post(&db.pool, 42, &[1]).await.is_err(), "поста 42 нет: ошибка");
        assert!(db.table("SELECT * FROM post_tags").await.is_empty(), "ничего не должно сохраниться");
    }
}
