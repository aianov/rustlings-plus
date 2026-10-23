// Два relay с SKIP LOCKED публикуют по одному событию за раз. Подписчики жалуются: иногда «пост удалён»
// приходит раньше «пост отредактирован». Пока первый relay держит событие 1 поста 5 и медленно его публикует,
// второй пропускает занятую строку и берёт следующее событие того же поста. Почини выборку: relay может взять
// событие, только если у этого поста нет более ранних неотправленных событий. События разных постов
// по-прежнему идут параллельно.

use sqlx::PgPool;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

#[derive(Debug, Clone, PartialEq)]
struct Event {
    id: i64,
    aggregate_id: i64,
    kind: String,
}

trait Publisher: Send + Sync {
    fn publish(&self, event: &Event) -> Result<(), String>;
}

async fn relay_one(db: &PgPool, bus: &dyn Publisher) -> Result<bool, BoxError> {
    let mut tx = db.begin().await?;
    let row = sqlx::query_as::<_, (i64, i64, String)>(
        "SELECT id, aggregate_id, kind FROM outbox WHERE sent_at IS NULL ORDER BY id LIMIT 1 FOR UPDATE SKIP LOCKED",
    )
    .fetch_optional(&mut *tx)
    .await?;
    let Some((id, aggregate_id, kind)) = row else {
        tx.rollback().await?;
        return Ok(false);
    };
    bus.publish(&Event { id, aggregate_id, kind })?;
    sqlx::query("UPDATE outbox SET sent_at = now() WHERE id = $1").bind(id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(true)
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    use std::sync::Mutex;

    #[derive(Default)]
    struct Bus {
        sent: Mutex<Vec<Event>>,
        fail_on: Mutex<Option<usize>>,
        delay_ms: u64,
        slow_created: bool,
    }

    impl Publisher for Bus {
        fn publish(&self, event: &Event) -> Result<(), String> {
            if event.kind.is_empty() {
                return Err("событие без типа".to_string());
            }
            let delay = if self.slow_created && event.kind == "post_created" { 200 } else { self.delay_ms };
            if delay > 0 {
                std::thread::sleep(std::time::Duration::from_millis(delay));
            }
            let mut sent = self.sent.lock().unwrap();
            if *self.fail_on.lock().unwrap() == Some(sent.len()) {
                *self.fail_on.lock().unwrap() = None;
                return Err("брокер недоступен".to_string());
            }
            sent.push(event.clone());
            Ok(())
        }
    }

    const SEED: &str = "CREATE TABLE posts (id bigint PRIMARY KEY, author bigint NOT NULL, text text NOT NULL CHECK (length(text) > 0)); CREATE TABLE outbox (id bigserial PRIMARY KEY, aggregate_id bigint NOT NULL, kind text NOT NULL, payload jsonb NOT NULL DEFAULT '{}', sent_at timestamptz); INSERT INTO outbox (aggregate_id, kind) VALUES (5, 'post_created'), (6, 'post_created'), (5, 'post_edited'), (6, 'post_liked'), (5, 'post_deleted'), (7, 'post_created'), (6, 'post_deleted')";

    #[tokio::test]
    async fn busy_event_holds_back_only_its_post() {
        let db = TestDb::with_sql(SEED).await;
        let bus = Bus::default();
        let mut holder = db.pool.begin().await.expect("транзакция");
        sqlx::query("SELECT id FROM outbox WHERE id = 1 FOR UPDATE").execute(&mut *holder).await.expect("другой relay держит событие 1 поста 5");
        while relay_one(&db.pool, &bus).await.expect("relay") {}
        let first: Vec<i64> = bus.sent.lock().unwrap().iter().map(|e| e.aggregate_id).collect();
        assert!(!first.contains(&5), "пока первое событие поста 5 у другого relay, следующие события поста 5 ждут: {first:?}");
        assert!(first.contains(&6) && first.contains(&7), "события других постов идут без задержки: {first:?}");
        holder.rollback().await.expect("откат");
        while relay_one(&db.pool, &bus).await.expect("relay") {}
        let sent = bus.sent.lock().unwrap();
        assert_eq!(sent.len(), 7, "все семь событий опубликованы");
        for post in [5, 6, 7] {
            let ids: Vec<i64> = sent.iter().filter(|e| e.aggregate_id == post).map(|e| e.id).collect();
            let mut sorted = ids.clone();
            sorted.sort_unstable();
            assert_eq!(ids, sorted, "события поста {post} опубликованы в порядке записи: {ids:?}");
        }
    }
}
