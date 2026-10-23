use sqlx::PgPool;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Diagnosis {
    pub waiting: Vec<String>,
    pub blockers: Vec<String>,
    pub idle_in_tx: Vec<String>,
    pub horizon_holders: Vec<String>,
}

const HOLDERS: &str = "state LIKE 'idle in transaction%' AND (backend_xmin IS NOT NULL OR backend_xid IS NOT NULL)";

pub async fn diagnose(pool: &PgPool, app_prefix: &str) -> Result<Diagnosis, sqlx::Error> {
    let waiting: Vec<String> = sqlx::query_scalar(
        "SELECT application_name FROM pg_stat_activity
         WHERE application_name LIKE $1 || '%' AND pid <> pg_backend_pid() AND cardinality(pg_blocking_pids(pid)) > 0
         ORDER BY 1",
    )
    .bind(app_prefix)
    .fetch_all(pool)
    .await?;
    let blockers: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT b.application_name
         FROM pg_stat_activity w
         CROSS JOIN LATERAL unnest(pg_blocking_pids(w.pid)) AS bp(pid)
         JOIN pg_stat_activity b ON b.pid = bp.pid
         WHERE w.application_name LIKE $1 || '%' AND w.pid <> pg_backend_pid()
         ORDER BY 1",
    )
    .bind(app_prefix)
    .fetch_all(pool)
    .await?;
    let idle_in_tx: Vec<String> = sqlx::query_scalar(
        "SELECT application_name FROM pg_stat_activity
         WHERE application_name LIKE $1 || '%' AND pid <> pg_backend_pid() AND state LIKE 'idle in transaction%'
         ORDER BY 1",
    )
    .bind(app_prefix)
    .fetch_all(pool)
    .await?;
    let horizon_holders: Vec<String> = sqlx::query_scalar(&format!(
        "SELECT application_name FROM pg_stat_activity WHERE application_name LIKE $1 || '%' AND pid <> pg_backend_pid() AND {HOLDERS} ORDER BY 1"
    ))
    .bind(app_prefix)
    .fetch_all(pool)
    .await?;
    Ok(Diagnosis { waiting, blockers, idle_in_tx, horizon_holders })
}

pub async fn resolve(pool: &PgPool, app_prefix: &str) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar(&format!(
        "WITH victims AS (
             SELECT pid, application_name FROM pg_stat_activity
             WHERE application_name LIKE $1 || '%' AND pid <> pg_backend_pid()
               AND (pid IN (SELECT unnest(pg_blocking_pids(w.pid)) FROM pg_stat_activity w WHERE w.application_name LIKE $1 || '%')
                    OR ({HOLDERS}))
         )
         SELECT application_name FROM victims WHERE pg_terminate_backend(pid) ORDER BY 1"
    ))
    .bind(app_prefix)
    .fetch_all(pool)
    .await
}

pub async fn vacuum_bloated(pool: &PgPool, min_dead_pct: i32) -> Result<Vec<String>, sqlx::Error> {
    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT relname::text FROM pg_stat_user_tables
         WHERE schemaname = current_schema()
           AND round(100.0 * n_dead_tup / greatest(n_live_tup + n_dead_tup, 1)) >= $1
         ORDER BY 1",
    )
    .bind(min_dead_pct)
    .fetch_all(pool)
    .await?;
    for table in &tables {
        sqlx::raw_sql(&format!("VACUUM \"{table}\"")).execute(pool).await?;
    }
    Ok(tables)
}

pub async fn harden_role(pool: &PgPool, role: &str, password: &str) -> Result<(), sqlx::Error> {
    let schema: String = sqlx::query_scalar("SELECT current_schema()").fetch_one(pool).await?;
    sqlx::raw_sql(&format!(
        "CREATE ROLE \"{role}\" LOGIN PASSWORD '{password}';
         ALTER ROLE \"{role}\" SET statement_timeout = '1s';
         ALTER ROLE \"{role}\" SET idle_in_transaction_session_timeout = '2s';
         GRANT USAGE ON SCHEMA \"{schema}\" TO \"{role}\";
         GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA \"{schema}\" TO \"{role}\";
         GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA \"{schema}\" TO \"{role}\";
         ALTER DEFAULT PRIVILEGES IN SCHEMA \"{schema}\" GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO \"{role}\";
         ALTER DEFAULT PRIVILEGES IN SCHEMA \"{schema}\" GRANT USAGE, SELECT ON SEQUENCES TO \"{role}\""
    ))
    .execute(pool)
    .await?;
    Ok(())
}
