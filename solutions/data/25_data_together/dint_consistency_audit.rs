use serde_json::json;
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

async fn create_post(db: &PgPool, id: i64, author: i64, text: &str) -> Result<(), BoxError> {
    let mut tx = db.begin().await?;
    sqlx::query("INSERT INTO posts (id, author, text) VALUES ($1, $2, $3)").bind(id).bind(author).bind(text).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO outbox (aggregate_id, kind, payload) VALUES ($1, 'post_created', $2)")
        .bind(id)
        .bind(json!({ "author": author }))
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

async fn relay_one(db: &PgPool, bus: &dyn Publisher) -> Result<bool, BoxError> {
    let mut tx = db.begin().await?;
    let row = sqlx::query_as::<_, (i64, i64, String)>(
        "SELECT id, aggregate_id, kind FROM outbox o
         WHERE sent_at IS NULL
           AND NOT EXISTS (SELECT 1 FROM outbox e WHERE e.aggregate_id = o.aggregate_id AND e.sent_at IS NULL AND e.id < o.id)
         ORDER BY id LIMIT 1 FOR UPDATE SKIP LOCKED",
    )
    .fetch_optional(&mut *tx)
    .await?;
    let Some((id, aggregate_id, kind)) = row else {
        tx.rollback().await?;
        return Ok(false);
    };
    if let Err(e) = bus.publish(&Event { id, aggregate_id, kind }) {
        tx.rollback().await?;
        return Err(e.into());
    }
    sqlx::query("UPDATE outbox SET sent_at = now() WHERE id = $1").bind(id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(true)
}

async fn handle_like(db: &PgPool, event_id: i64, post: i64) -> Result<bool, BoxError> {
    let mut tx = db.begin().await?;
    let fresh = sqlx::query("INSERT INTO inbox (event_id) VALUES ($1) ON CONFLICT DO NOTHING").bind(event_id).execute(&mut *tx).await?.rows_affected() == 1;
    if !fresh {
        tx.rollback().await?;
        return Ok(false);
    }
    sqlx::query("UPDATE posts SET likes = likes + 1 WHERE id = $1").bind(post).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(true)
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    use std::sync::{Arc, Mutex};

    const SCHEMA: &str = "CREATE TABLE posts (id bigint PRIMARY KEY, author bigint NOT NULL, text text NOT NULL CHECK (length(text) > 0), likes bigint NOT NULL DEFAULT 0); CREATE TABLE outbox (id bigserial PRIMARY KEY, aggregate_id bigint NOT NULL, kind text NOT NULL, payload jsonb NOT NULL DEFAULT '{}', sent_at timestamptz); CREATE TABLE inbox (event_id bigint PRIMARY KEY)";

    #[derive(Default)]
    struct Bus {
        sent: Mutex<Vec<Event>>,
        failures: Mutex<usize>,
    }

    impl Publisher for Bus {
        fn publish(&self, event: &Event) -> Result<(), String> {
            let mut failures = self.failures.lock().unwrap();
            if *failures > 0 {
                *failures -= 1;
                return Err("брокер недоступен".to_string());
            }
            if event.kind.is_empty() {
                return Err("событие без типа".to_string());
            }
            self.sent.lock().unwrap().push(event.clone());
            Ok(())
        }
    }

    #[tokio::test]
    async fn no_event_without_a_post() {
        let db = TestDb::with_sql(SCHEMA).await;
        assert!(create_post(&db.pool, 1, 7, "").await.is_err(), "пустой текст нарушает CHECK");
        let t = db.table("SELECT id FROM outbox").await;
        assert!(t.is_empty(), "события о несохранённом посте быть не должно\n{t}");
        create_post(&db.pool, 2, 7, "пост").await.expect("пост");
        assert_eq!(db.table("SELECT count(*) FROM outbox").await.cell(0, 0), "1");
    }

    #[tokio::test]
    async fn broker_outage_loses_nothing() {
        let db = TestDb::with_sql(SCHEMA).await;
        create_post(&db.pool, 1, 7, "пост").await.expect("пост");
        let bus = Bus::default();
        *bus.failures.lock().unwrap() = 1;
        assert!(relay_one(&db.pool, &bus).await.is_err(), "брокер лежит, публикация не удалась");
        assert!(relay_one(&db.pool, &bus).await.expect("повтор"), "после восстановления событие уходит");
        assert_eq!(bus.sent.lock().unwrap().len(), 1, "событие не потерялось");
        assert!(!relay_one(&db.pool, &bus).await.expect("пусто"), "больше отправлять нечего");
    }

    #[tokio::test]
    async fn busy_event_holds_back_only_its_post() {
        let db = TestDb::with_sql(SCHEMA).await;
        db.run("INSERT INTO outbox (aggregate_id, kind) VALUES (5, 'post_created'), (6, 'post_created'), (5, 'post_edited'), (6, 'post_liked'), (5, 'post_deleted')").await;
        let bus = Bus::default();
        let mut holder = db.pool.begin().await.expect("транзакция");
        sqlx::query("SELECT id FROM outbox WHERE id = 1 FOR UPDATE").execute(&mut *holder).await.expect("другой relay держит событие 1");
        let mut rounds = 0;
        while tokio::time::timeout(std::time::Duration::from_secs(3), relay_one(&db.pool, &bus)).await.expect("relay не должен ждать чужую блокировку").expect("relay") {
            rounds += 1;
            assert!(rounds < 20, "relay крутится без конца");
        }
        let first: Vec<i64> = bus.sent.lock().unwrap().iter().map(|e| e.aggregate_id).collect();
        assert_eq!(first, [6, 6], "события поста 5 ждут, пока его первое событие у другого relay: {first:?}");
        holder.rollback().await.expect("откат");
        while relay_one(&db.pool, &bus).await.expect("relay") {}
        let ids: Vec<i64> = bus.sent.lock().unwrap().iter().filter(|e| e.aggregate_id == 5).map(|e| e.id).collect();
        assert_eq!(ids, [1, 3, 5], "события поста 5 по порядку");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn two_relays_never_duplicate() {
        let db = TestDb::with_sql(SCHEMA).await;
        db.run("INSERT INTO outbox (aggregate_id, kind) SELECT g, 'post_created' FROM generate_series(1, 40) g").await;
        let bus = Arc::new(Bus::default());
        let mut relays = Vec::new();
        for _ in 0..2 {
            let (pool, bus) = (db.pool.clone(), Arc::clone(&bus));
            relays.push(tokio::spawn(async move {
                while relay_one(&pool, bus.as_ref()).await.map_err(|e| e.to_string())? {}
                Ok::<(), String>(())
            }));
        }
        for r in relays {
            r.await.expect("задача").expect("relay");
        }
        assert_eq!(bus.sent.lock().unwrap().len(), 40, "каждое событие ровно один раз");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn duplicate_likes_count_once() {
        let db = TestDb::with_sql(SCHEMA).await;
        db.run("INSERT INTO posts (id, author, text) VALUES (1, 7, 'пост')").await;
        assert!(handle_like(&db.pool, 100, 1).await.expect("лайк"));
        assert!(!handle_like(&db.pool, 100, 1).await.expect("дубль"), "дубль пропускается");
        let mut tasks = Vec::new();
        for _ in 0..8 {
            let pool = db.pool.clone();
            tasks.push(tokio::spawn(async move { handle_like(&pool, 200, 1).await.map_err(|e| e.to_string()) }));
        }
        for t in tasks {
            t.await.expect("задача").expect("лайк");
        }
        assert_eq!(db.table("SELECT likes FROM posts").await.cell(0, 0), "2", "два разных события, два лайка");
    }
}
