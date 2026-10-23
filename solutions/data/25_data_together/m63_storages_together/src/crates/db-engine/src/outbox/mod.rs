use contracts::{Cause, DomainEvent, Fact, OutboxRow, fresh_event_id};
use sqlx::{PgConnection, PgPool, Row};

use crate::error::DbError;

pub const OUTBOX_CLAIMS_SQL: &str = "
ALTER TABLE outbox ADD COLUMN IF NOT EXISTS claimed_by TEXT;
ALTER TABLE outbox ADD COLUMN IF NOT EXISTS claimed_until TIMESTAMPTZ;
CREATE INDEX IF NOT EXISTS outbox_by_aggregate ON outbox ((NULLIF(event, '')::jsonb ->> 'aggregate_id'), seq) WHERE NOT published;
CREATE OR REPLACE FUNCTION outbox_wake() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    PERFORM pg_notify('outbox_' || current_schema(), '');
    RETURN NULL;
END $$;
DROP TRIGGER IF EXISTS outbox_wake ON outbox;
CREATE TRIGGER outbox_wake AFTER INSERT ON outbox FOR EACH STATEMENT EXECUTE FUNCTION outbox_wake();
";

const CLAIM: &str = "
UPDATE outbox SET claimed_by = $2, claimed_until = now() + make_interval(secs => $3)
WHERE seq IN (
    SELECT o.seq FROM outbox o
    WHERE NOT o.published
      AND (o.claimed_until IS NULL OR o.claimed_until < now() OR o.claimed_by = $2)
      AND NOT EXISTS (
          SELECT 1 FROM outbox e
          WHERE (NULLIF(e.event, '')::jsonb ->> 'aggregate_id') = (NULLIF(o.event, '')::jsonb ->> 'aggregate_id')
            AND e.seq < o.seq AND NOT e.published
      )
    ORDER BY o.seq
    LIMIT $1
    FOR UPDATE SKIP LOCKED
)
RETURNING seq, event, request_id, trace_id";

const RELEASE: &str = "UPDATE outbox SET published = true, claimed_by = NULL WHERE seq <= $1 AND claimed_by = $2 AND NOT published";

pub const OUTBOX_SQL: &str = "
CREATE TABLE IF NOT EXISTS outbox_versions (
    aggregate TEXT PRIMARY KEY,
    version BIGINT NOT NULL
);
CREATE TABLE IF NOT EXISTS outbox (
    seq BIGSERIAL PRIMARY KEY,
    event TEXT NOT NULL,
    request_id TEXT NOT NULL,
    trace_id TEXT,
    published BOOLEAN NOT NULL DEFAULT false
);
CREATE INDEX IF NOT EXISTS outbox_waiting ON outbox (seq) WHERE NOT published;
";

const NEXT_VERSION: &str = "
INSERT INTO outbox_versions (aggregate, version) VALUES ($1, 1)
ON CONFLICT (aggregate) DO UPDATE SET version = outbox_versions.version + 1
RETURNING version";

const PUT_EVENT: &str = "
INSERT INTO outbox (event, request_id, trace_id) VALUES ('', $1, $2) RETURNING seq";

const FILL_EVENT: &str = "UPDATE outbox SET event = $2 WHERE seq = $1";

const UNPUBLISHED: &str = "
SELECT seq, event, request_id, trace_id FROM outbox WHERE NOT published ORDER BY seq LIMIT $1";

const MARK_PUBLISHED: &str = "UPDATE outbox SET published = true WHERE seq <= $1 AND NOT published";

const WAITING: &str = "SELECT count(*) FROM outbox WHERE NOT published";

fn to_u64(value: i64) -> u64 {
    u64::try_from(value).unwrap_or(0)
}

pub async fn record_fact(
    conn: &mut PgConnection,
    fact: &Fact,
    cause: &Cause,
) -> Result<OutboxRow, DbError> {
    let version: i64 = sqlx::query_scalar(NEXT_VERSION)
        .bind(fact.aggregate())
        .fetch_one(&mut *conn)
        .await?;
    let seq: i64 = sqlx::query_scalar(PUT_EVENT)
        .bind(&cause.origin.request_id)
        .bind(&cause.origin.trace_id)
        .fetch_one(&mut *conn)
        .await?;
    let event = DomainEvent::new(&fresh_event_id(), fact, to_u64(version), cause.at);
    let text = serde_json::to_string(&event).map_err(|error| DbError::Query(error.to_string()))?;
    sqlx::query(FILL_EVENT)
        .bind(seq)
        .bind(text)
        .execute(&mut *conn)
        .await?;
    Ok(OutboxRow {
        seq: to_u64(seq),
        event,
        request_id: cause.origin.request_id.clone(),
        trace_id: cause.origin.trace_id.clone(),
    })
}

pub async fn unpublished(pool: &PgPool, limit: usize) -> Result<Vec<OutboxRow>, DbError> {
    let rows = sqlx::query(UNPUBLISHED)
        .bind(i64::try_from(limit).unwrap_or(i64::MAX))
        .fetch_all(pool)
        .await?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let seq: i64 = row.try_get("seq")?;
        let event: String = row.try_get("event")?;
        let Ok(event) = serde_json::from_str::<DomainEvent>(&event) else {
            continue;
        };
        out.push(OutboxRow {
            seq: to_u64(seq),
            event,
            request_id: row.try_get("request_id")?,
            trace_id: row.try_get("trace_id")?,
        });
    }
    Ok(out)
}

pub async fn mark_published(pool: &PgPool, through: u64) -> Result<(), DbError> {
    sqlx::query(MARK_PUBLISHED)
        .bind(i64::try_from(through).unwrap_or(i64::MAX))
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn waiting(pool: &PgPool) -> Result<usize, DbError> {
    let count: i64 = sqlx::query_scalar(WAITING).fetch_one(pool).await?;
    Ok(usize::try_from(count).unwrap_or(0))
}

pub async fn claim(
    pool: &PgPool,
    owner: &str,
    limit: usize,
    hold: std::time::Duration,
) -> Result<Vec<OutboxRow>, DbError> {
    let rows = sqlx::query(CLAIM)
        .bind(i64::try_from(limit).unwrap_or(i64::MAX))
        .bind(owner)
        .bind(hold.as_secs_f64())
        .fetch_all(pool)
        .await?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let seq: i64 = row.try_get("seq")?;
        let event: String = row.try_get("event")?;
        let Ok(event) = serde_json::from_str::<DomainEvent>(&event) else {
            continue;
        };
        out.push(OutboxRow {
            seq: to_u64(seq),
            event,
            request_id: row.try_get("request_id")?,
            trace_id: row.try_get("trace_id")?,
        });
    }
    out.sort_by_key(|row| row.seq);
    Ok(out)
}

pub async fn release(pool: &PgPool, owner: &str, through: u64) -> Result<(), DbError> {
    sqlx::query(RELEASE)
        .bind(i64::try_from(through).unwrap_or(i64::MAX))
        .bind(owner)
        .execute(pool)
        .await?;
    Ok(())
}

pub fn wake_on_outbox(pool: PgPool, signal: std::sync::Arc<tokio::sync::Notify>) {
    tokio::spawn(async move {
        loop {
            let schema: Result<String, sqlx::Error> = sqlx::query_scalar("SELECT current_schema()")
                .fetch_one(&pool)
                .await;
            let listened = match schema {
                Ok(schema) => listen(&pool, &format!("outbox_{schema}"), &signal).await,
                Err(error) => Err(error),
            };
            if listened.is_err() && pool.is_closed() {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    });
}

async fn listen(
    pool: &PgPool,
    channel: &str,
    signal: &tokio::sync::Notify,
) -> Result<(), sqlx::Error> {
    let mut listener = sqlx::postgres::PgListener::connect_with(pool).await?;
    listener.listen(channel).await?;
    loop {
        listener.recv().await?;
        signal.notify_one();
    }
}
