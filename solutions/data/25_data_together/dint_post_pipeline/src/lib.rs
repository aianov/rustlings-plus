use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use scylla::Session;
use serde_json::{Value, json};
use sqlx::PgPool;

pub type BoxError = Box<dyn std::error::Error + Send + Sync>;

#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    pub id: i64,
    pub aggregate_id: i64,
    pub kind: String,
    pub payload: Value,
}

pub trait Sink: Send + Sync {
    fn publish(&self, event: &Event) -> Result<(), String>;
}

pub struct Pipeline {
    pg: PgPool,
    scylla: Arc<Session>,
    keyspace: String,
}

fn now_us() -> i64 {
    let us = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_micros()).unwrap_or(0);
    i64::try_from(us).unwrap_or(i64::MAX)
}

const PG_SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS posts (id bigserial PRIMARY KEY, author bigint NOT NULL, text text NOT NULL CHECK (length(text) > 0), deleted boolean NOT NULL DEFAULT false);
CREATE TABLE IF NOT EXISTS follows (follower bigint NOT NULL, author bigint NOT NULL, PRIMARY KEY (follower, author));
CREATE TABLE IF NOT EXISTS outbox (id bigserial PRIMARY KEY, aggregate_id bigint NOT NULL, kind text NOT NULL, payload jsonb NOT NULL, sent_at timestamptz);
CREATE TABLE IF NOT EXISTS inbox (event_id bigint PRIMARY KEY);
";

impl Pipeline {
    pub async fn new(pg: PgPool, scylla: Arc<Session>, keyspace: &str) -> Result<Pipeline, BoxError> {
        sqlx::raw_sql(PG_SCHEMA).execute(&pg).await?;
        scylla
            .query(
                format!(
                    "CREATE TABLE IF NOT EXISTS {keyspace}.feed (user_id bigint, post_id bigint, author bigint, text text, PRIMARY KEY (user_id, post_id)) WITH CLUSTERING ORDER BY (post_id DESC)"
                ),
                (),
            )
            .await?;
        scylla.await_schema_agreement().await?;
        Ok(Pipeline { pg, scylla, keyspace: keyspace.to_string() })
    }

    pub async fn follow(&self, follower: i64, author: i64) -> Result<(), BoxError> {
        sqlx::query("INSERT INTO follows (follower, author) VALUES ($1, $2) ON CONFLICT DO NOTHING")
            .bind(follower)
            .bind(author)
            .execute(&self.pg)
            .await?;
        Ok(())
    }

    pub async fn create_post(&self, author: i64, text: &str) -> Result<i64, BoxError> {
        let mut tx = self.pg.begin().await?;
        let id: i64 = sqlx::query_scalar("INSERT INTO posts (author, text) VALUES ($1, $2) RETURNING id")
            .bind(author)
            .bind(text)
            .fetch_one(&mut *tx)
            .await?;
        let payload = json!({ "author": author, "text": text, "version_us": now_us() });
        sqlx::query("INSERT INTO outbox (aggregate_id, kind, payload) VALUES ($1, 'post_created', $2)")
            .bind(id)
            .bind(payload)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(id)
    }

    pub async fn delete_post(&self, post: i64) -> Result<(), BoxError> {
        let mut tx = self.pg.begin().await?;
        let author: Option<i64> = sqlx::query_scalar("UPDATE posts SET deleted = true WHERE id = $1 AND NOT deleted RETURNING author")
            .bind(post)
            .fetch_optional(&mut *tx)
            .await?;
        let Some(author) = author else {
            tx.rollback().await?;
            return Ok(());
        };
        sqlx::query("INSERT INTO outbox (aggregate_id, kind, payload) VALUES ($1, 'post_deleted', $2)")
            .bind(post)
            .bind(json!({ "author": author, "version_us": now_us() }))
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn relay_once(&self, sink: &dyn Sink, batch: i64) -> Result<usize, BoxError> {
        let mut tx = self.pg.begin().await?;
        let rows = sqlx::query_as::<_, (i64, i64, String, Value)>(
            "SELECT id, aggregate_id, kind, payload FROM outbox o
             WHERE sent_at IS NULL
               AND NOT EXISTS (SELECT 1 FROM outbox e WHERE e.aggregate_id = o.aggregate_id AND e.sent_at IS NULL AND e.id < o.id)
             ORDER BY id LIMIT $1 FOR UPDATE SKIP LOCKED",
        )
        .bind(batch)
        .fetch_all(&mut *tx)
        .await?;
        let mut sent = Vec::new();
        for (id, aggregate_id, kind, payload) in rows {
            if sink.publish(&Event { id, aggregate_id, kind, payload }).is_err() {
                break;
            }
            sent.push(id);
        }
        sqlx::query("UPDATE outbox SET sent_at = now() WHERE id = ANY($1)").bind(&sent).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(sent.len())
    }

    pub async fn consume(&self, event: &Event) -> Result<bool, BoxError> {
        let mut tx = self.pg.begin().await?;
        let fresh = sqlx::query("INSERT INTO inbox (event_id) VALUES ($1) ON CONFLICT DO NOTHING")
            .bind(event.id)
            .execute(&mut *tx)
            .await?
            .rows_affected()
            == 1;
        if !fresh {
            tx.rollback().await?;
            return Ok(false);
        }
        let author = event.payload.get("author").and_then(Value::as_i64).ok_or("в событии нет автора")?;
        let version = event.payload.get("version_us").and_then(Value::as_i64).ok_or("в событии нет версии")?;
        let mut readers: Vec<i64> = sqlx::query_scalar("SELECT follower FROM follows WHERE author = $1").bind(author).fetch_all(&mut *tx).await?;
        readers.push(author);
        let ks = &self.keyspace;
        match event.kind.as_str() {
            "post_created" => {
                let text = event.payload.get("text").and_then(Value::as_str).unwrap_or_default();
                for user in &readers {
                    self.scylla
                        .query(
                            format!("INSERT INTO {ks}.feed (user_id, post_id, author, text) VALUES (?, ?, ?, ?) USING TIMESTAMP ?"),
                            (user, event.aggregate_id, author, text, version),
                        )
                        .await?;
                }
            }
            "post_deleted" => {
                for user in &readers {
                    self.scylla
                        .query(format!("DELETE FROM {ks}.feed USING TIMESTAMP ? WHERE user_id = ? AND post_id = ?"), (version, user, event.aggregate_id))
                        .await?;
                }
            }
            _ => {}
        }
        tx.commit().await?;
        Ok(true)
    }

    pub async fn feed(&self, user: i64) -> Result<Vec<(i64, String)>, BoxError> {
        let res = self.scylla.query(format!("SELECT post_id, text FROM {}.feed WHERE user_id = ?", self.keyspace), (user,)).await?;
        Ok(res.rows_typed::<(i64, String)>()?.collect::<Result<Vec<_>, _>>()?)
    }
}
