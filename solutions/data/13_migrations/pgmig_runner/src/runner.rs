use std::time::Duration;

use sqlx::{PgConnection, PgPool};

use crate::{MigrateError, Migration, Options, Report, State, Status};

const CREATE_LOG: &str = "CREATE TABLE IF NOT EXISTS schema_migrations \
    (version bigint PRIMARY KEY, name text NOT NULL, checksum text NOT NULL, applied_at timestamptz NOT NULL DEFAULT now())";
const LIST_LOG: &str = "SELECT version, checksum FROM schema_migrations ORDER BY version";
const LIST_VERSIONS: &str = "SELECT version FROM schema_migrations";
const RECORD: &str = "INSERT INTO schema_migrations (version, name, checksum) VALUES ($1, $2, md5($3))";
const CHECKSUM: &str = "SELECT md5($1)";
const SET_LOCK_TIMEOUT: &str = "SELECT set_config('lock_timeout', $1, $2)";
const RESET_LOCK_TIMEOUT: &str = "RESET lock_timeout";
const LOCK: &str = "SELECT pg_advisory_lock($1)";
const UNLOCK: &str = "SELECT pg_advisory_unlock($1)";
const LOG_EXISTS: &str = "SELECT to_regclass('schema_migrations') IS NOT NULL";
const NO_TRANSACTION: &str = "-- no-transaction";
const LOCK_NOT_AVAILABLE: &str = "55P03";

pub struct Migrator {
    pool: PgPool,
    options: Options,
}

impl Migrator {
    pub fn new(pool: PgPool, options: Options) -> Migrator {
        Migrator { pool, options }
    }

    pub async fn run(&self, migrations: &[Migration]) -> Result<Report, MigrateError> {
        let mut lock = self.pool.acquire().await?;
        sqlx::query(LOCK).bind(self.options.lock_key).execute(&mut *lock).await?;
        let outcome = self.run_locked(migrations).await;
        let unlock = sqlx::query(UNLOCK).bind(self.options.lock_key).execute(&mut *lock).await;
        let report = outcome?;
        unlock?;
        Ok(report)
    }

    pub async fn status(&self, migrations: &[Migration]) -> Result<Vec<Status>, MigrateError> {
        let exists: bool = sqlx::query_scalar(LOG_EXISTS).fetch_one(&self.pool).await?;
        let done: Vec<i64> = if exists { sqlx::query_scalar(LIST_VERSIONS).fetch_all(&self.pool).await? } else { Vec::new() };
        let mut all: Vec<Status> = migrations
            .iter()
            .map(|m| Status {
                version: m.version,
                name: m.name.clone(),
                state: if done.contains(&m.version) { State::Applied } else { State::Pending },
            })
            .collect();
        all.sort_by_key(|s| s.version);
        Ok(all)
    }

    async fn run_locked(&self, migrations: &[Migration]) -> Result<Report, MigrateError> {
        sqlx::raw_sql(CREATE_LOG).execute(&self.pool).await?;
        let mut sorted: Vec<&Migration> = migrations.iter().collect();
        sorted.sort_by_key(|m| m.version);
        let recorded: Vec<(i64, String)> = sqlx::query_as(LIST_LOG).fetch_all(&self.pool).await?;
        for (version, checksum) in &recorded {
            let file = sorted.iter().find(|m| m.version == *version).ok_or(MigrateError::MissingFile { version: *version })?;
            let current: String = sqlx::query_scalar(CHECKSUM).bind(&file.sql).fetch_one(&self.pool).await?;
            if &current != checksum {
                return Err(MigrateError::Drift { version: *version });
            }
        }
        let pending: Vec<&Migration> = sorted.iter().copied().filter(|m| !recorded.iter().any(|r| r.0 == m.version)).collect();
        if let Some(newest) = recorded.last().map(|r| r.0)
            && let Some(old) = pending.iter().find(|m| m.version < newest)
        {
            return Err(MigrateError::OutOfOrder { version: old.version });
        }
        let mut report = Report::default();
        for migration in pending {
            self.apply(migration).await?;
            report.applied.push(migration.version);
        }
        Ok(report)
    }

    async fn apply(&self, migration: &Migration) -> Result<(), MigrateError> {
        let mut attempt = 1;
        loop {
            match self.apply_once(migration).await {
                Ok(()) => return Ok(()),
                Err(sqlx::Error::Database(e)) if e.code().as_deref() == Some(LOCK_NOT_AVAILABLE) => {
                    if attempt >= self.options.attempts {
                        return Err(MigrateError::LockTimeout { version: migration.version });
                    }
                    attempt += 1;
                    tokio::time::sleep(Duration::from_millis(self.options.retry_pause_ms)).await;
                }
                Err(error) => return Err(MigrateError::Failed { version: migration.version, error }),
            }
        }
    }

    async fn apply_once(&self, migration: &Migration) -> Result<(), sqlx::Error> {
        let timeout = self.options.lock_timeout_ms.to_string();
        if migration.sql.trim_start().starts_with(NO_TRANSACTION) {
            let mut conn = self.pool.acquire().await?;
            sqlx::query(SET_LOCK_TIMEOUT).bind(&timeout).bind(false).execute(&mut *conn).await?;
            let result = apply_without_transaction(&mut conn, migration).await;
            let _ = sqlx::raw_sql(RESET_LOCK_TIMEOUT).execute(&mut *conn).await;
            result
        } else {
            let mut tx = self.pool.begin().await?;
            sqlx::query(SET_LOCK_TIMEOUT).bind(&timeout).bind(true).execute(&mut *tx).await?;
            sqlx::raw_sql(&migration.sql).execute(&mut *tx).await?;
            sqlx::query(RECORD).bind(migration.version).bind(&migration.name).bind(&migration.sql).execute(&mut *tx).await?;
            tx.commit().await
        }
    }
}

async fn apply_without_transaction(conn: &mut PgConnection, migration: &Migration) -> Result<(), sqlx::Error> {
    for statement in migration.sql.split(';').map(str::trim).filter(|s| !s.is_empty()) {
        sqlx::raw_sql(statement).execute(&mut *conn).await?;
    }
    sqlx::query(RECORD).bind(migration.version).bind(&migration.name).bind(&migration.sql).execute(&mut *conn).await?;
    Ok(())
}
