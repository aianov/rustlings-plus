use sqlx::PgPool;
use std::time::Duration;

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

async fn relay_once(db: &PgPool, bus: &dyn Publisher, batch: i64) -> Result<usize, BoxError> {
    let mut tx = db.begin().await?;
    let rows = sqlx::query_as::<_, (i64, i64, String)>(
        "SELECT id, aggregate_id, kind FROM outbox WHERE sent_at IS NULL ORDER BY id LIMIT $1 FOR UPDATE SKIP LOCKED",
    )
    .bind(batch)
    .fetch_all(&mut *tx)
    .await?;
    tokio::time::sleep(Duration::from_millis(20)).await;
    let mut sent = Vec::new();
    for (id, aggregate_id, kind) in rows {
        if bus.publish(&Event { id, aggregate_id, kind }).is_err() {
            break;
        }
        sent.push(id);
    }
    sqlx::query("UPDATE outbox SET sent_at = now() WHERE id = ANY($1)").bind(&sent).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(sent.len())
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

    use std::collections::HashSet;
    use std::sync::Arc;

    const SEED: &str = "CREATE TABLE posts (id bigint PRIMARY KEY, author bigint NOT NULL, text text NOT NULL CHECK (length(text) > 0)); CREATE TABLE outbox (id bigserial PRIMARY KEY, aggregate_id bigint NOT NULL, kind text NOT NULL, payload jsonb NOT NULL DEFAULT '{}', sent_at timestamptz)";

    async fn seed(db: &TestDb, n: i64) {
        db.run(&format!("INSERT INTO outbox (aggregate_id, kind) SELECT g, 'post_created' FROM generate_series(1, {n}) g")).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn two_relays_publish_each_event_once() {
        let db = TestDb::with_sql(SEED).await;
        seed(&db, 60).await;
        let bus = Arc::new(Bus::default());
        let mut relays = Vec::new();
        for _ in 0..2 {
            let (pool, bus) = (db.pool.clone(), Arc::clone(&bus));
            relays.push(tokio::spawn(async move {
                loop {
                    match relay_once(&pool, bus.as_ref(), 10).await {
                        Ok(0) => break Ok(()),
                        Ok(_) => {}
                        Err(e) => break Err(e.to_string()),
                    }
                }
            }));
        }
        for r in relays {
            r.await.expect("задача").expect("relay");
        }
        let sent = bus.sent.lock().unwrap();
        let unique: HashSet<i64> = sent.iter().map(|e| e.id).collect();
        assert_eq!(sent.len(), 60, "каждое из 60 событий опубликовано ровно один раз, а публикаций {}", sent.len());
        assert_eq!(unique.len(), 60);
    }

    #[tokio::test]
    async fn failed_publish_stays_unsent() {
        let db = TestDb::with_sql(SEED).await;
        seed(&db, 5).await;
        let bus = Bus::default();
        *bus.fail_on.lock().unwrap() = Some(2);
        assert_eq!(relay_once(&db.pool, &bus, 10).await.expect("relay"), 2, "брокер упал на третьем: опубликовано два");
        let left = db.table("SELECT count(*) FROM outbox WHERE sent_at IS NULL").await;
        assert_eq!(left.cell(0, 0), "3", "неопубликованные остались неотправленными\n{left}");
        assert_eq!(relay_once(&db.pool, &bus, 10).await.expect("relay"), 3, "следующий проход дошлёт остальные");
    }
}
