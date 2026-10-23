// Relay опрашивает outbox раз в 5 секунд, и событие о новом посте ждёт до пяти секунд, прежде чем уйти.
// Опрашивать чаще значит нагружать базу впустую. Postgres умеет будить слушателей: NOTIFY в транзакции
// доходит до тех, кто сделал LISTEN, сразу после коммита. Почини: `create_post` будит канал `channel` в той же
// транзакции, а `wait_for_work` возвращает true, как только пришло уведомление, и false, если за `poll`
// ничего не пришло (опрос остаётся страховкой).

use sqlx::postgres::PgListener;
use sqlx::PgPool;
use std::time::Duration;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

async fn create_post(db: &PgPool, channel: &str, id: i64, text: &str) -> Result<(), BoxError> {
    let mut tx = db.begin().await?;
    sqlx::query("INSERT INTO posts (id, author, text) VALUES ($1, 1, $2)").bind(id).bind(text).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO outbox (aggregate_id, kind) VALUES ($1, 'post_created')").bind(id).execute(&mut *tx).await?;
    let _ = channel;
    tx.commit().await?;
    Ok(())
}

async fn wait_for_work(listener: &mut PgListener, poll: Duration) -> Result<bool, BoxError> {
    tokio::time::sleep(poll).await;
    let _ = listener;
    Ok(false)
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    use sqlx::postgres::PgListener;
    use std::time::{Duration, Instant};

    const SEED: &str = "CREATE TABLE posts (id bigint PRIMARY KEY, author bigint NOT NULL, text text NOT NULL CHECK (length(text) > 0)); CREATE TABLE outbox (id bigserial PRIMARY KEY, aggregate_id bigint NOT NULL, kind text NOT NULL, payload jsonb NOT NULL DEFAULT '{}', sent_at timestamptz)";

    #[tokio::test]
    async fn relay_wakes_up_right_after_commit() {
        let db = TestDb::with_sql(SEED).await;
        let channel = format!("outbox_{}", db.schema());
        let mut listener = PgListener::connect_with(&db.pool).await.expect("слушатель");
        listener.listen(&channel).await.expect("LISTEN");
        let (pool, ch) = (db.pool.clone(), channel.clone());
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            create_post(&pool, &ch, 1, "привет").await.map_err(|e| e.to_string())
        });
        let start = Instant::now();
        assert!(wait_for_work(&mut listener, Duration::from_secs(5)).await.expect("ожидание"), "уведомление должно прийти");
        assert!(start.elapsed() < Duration::from_secs(2), "relay проснулся сразу после коммита, а не по опросу: {:?}", start.elapsed());
    }

    #[tokio::test]
    async fn quiet_poll_returns_false() {
        let db = TestDb::with_sql(SEED).await;
        let mut listener = PgListener::connect_with(&db.pool).await.expect("слушатель");
        listener.listen(&format!("outbox_{}", db.schema())).await.expect("LISTEN");
        let start = Instant::now();
        assert!(!wait_for_work(&mut listener, Duration::from_millis(200)).await.expect("ожидание"), "без новых событий срабатывает опрос");
        assert!(start.elapsed() >= Duration::from_millis(200));
    }
}
