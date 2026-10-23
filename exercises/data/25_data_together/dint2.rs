// Outbox: событие записывается в таблицу outbox той же транзакцией, что и пост. Либо есть и пост, и событие,
// либо нет ни того, ни другого. Отправкой в брокер займётся отдельный процесс (relay) в следующей задаче.
// TODO: `create_post` пишет пост и событие `post_created` (aggregate_id это id поста, payload это JSON с
// author и text) одной транзакцией.

use serde_json::json;
use sqlx::PgPool;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

async fn create_post(db: &PgPool, id: i64, author: i64, text: &str) -> Result<(), BoxError> {
    let _ = (db, id, author, text, json!({}));
    todo!("пост и событие одной транзакцией")
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = "CREATE TABLE posts (id bigint PRIMARY KEY, author bigint NOT NULL, text text NOT NULL CHECK (length(text) > 0)); CREATE TABLE outbox (id bigserial PRIMARY KEY, aggregate_id bigint NOT NULL, kind text NOT NULL, payload jsonb NOT NULL DEFAULT '{}', sent_at timestamptz)";

    #[tokio::test]
    async fn post_and_event_together() {
        let db = TestDb::with_sql(SEED).await;
        create_post(&db.pool, 1, 7, "привет").await.expect("пост");
        let t = db.table("SELECT aggregate_id, kind, payload->>'author', payload->>'text', sent_at IS NULL FROM outbox").await;
        assert_eq!(t.rows, [["1", "post_created", "7", "привет", "true"]], "событие о посте в outbox, ещё не отправлено\n{t}");
    }

    #[tokio::test]
    async fn neither_without_the_other() {
        let db = TestDb::with_sql(SEED).await;
        assert!(create_post(&db.pool, 2, 7, "").await.is_err(), "пустой текст нарушает CHECK");
        assert!(db.table("SELECT id FROM posts").await.is_empty(), "поста нет");
        let t = db.table("SELECT id FROM outbox").await;
        assert!(t.is_empty(), "и события о нём нет\n{t}");
    }
}
