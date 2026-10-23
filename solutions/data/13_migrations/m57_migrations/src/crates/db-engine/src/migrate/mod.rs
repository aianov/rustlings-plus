mod checksum;

use std::time::Duration;

use sqlx::pool::PoolConnection;
use sqlx::{Connection, Executor, PgConnection, PgPool, Postgres};

use crate::error::DbError;

pub use checksum::checksum;

pub type Migration = (&'static str, &'static str);

pub const NO_TRANSACTION: &str = "-- migrate: no-transaction";

pub const LOCK_TIMEOUT: Duration = Duration::from_secs(2);

pub const LOCK_ATTEMPTS: u32 = 5;

pub const LOCK_PAUSE: Duration = Duration::from_millis(150);

const VERSIONS: &str = "CREATE TABLE IF NOT EXISTS schema_migrations (version INTEGER PRIMARY KEY, name TEXT NOT NULL, checksum TEXT NOT NULL, applied_at TIMESTAMPTZ NOT NULL DEFAULT now())";
const LOCK: &str = "SELECT pg_advisory_lock(hashtext(current_schema()))";
const UNLOCK: &str = "SELECT pg_advisory_unlock(hashtext(current_schema()))";
const KNOWN: &str = "SELECT version, name, checksum FROM schema_migrations ORDER BY version";
const RECORD: &str = "INSERT INTO schema_migrations (version, name, checksum) VALUES ($1, $2, $3)";
const LOCAL_LOCK_TIMEOUT: &str = "SELECT set_config('lock_timeout', $1, true)";
const SESSION_LOCK_TIMEOUT: &str = "SELECT set_config('lock_timeout', $1, false)";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Applied {
    pub version: i32,
    pub name: String,
    pub checksum: String,
}

pub fn version(name: &str) -> Result<i32, DbError> {
    let digits: String = name.chars().take_while(char::is_ascii_digit).collect();
    match (digits.len(), digits.parse()) {
        (4, Ok(version)) => Ok(version),
        _ => Err(DbError::Migration(format!(
            "{name}: имя файла начинается с четырёх цифр версии, как 0001_init.sql"
        ))),
    }
}

fn named(name: &str) -> impl Fn(sqlx::Error) -> DbError + '_ {
    move |error| match DbError::from(error) {
        DbError::Query(why) => DbError::Query(format!("{name}: {why}")),
        other => other,
    }
}

pub const RETRY_MIGRATION: [&str; 2] = ["55P03", "40P01"];

fn locked_out(error: &sqlx::Error) -> bool {
    matches!(error, sqlx::Error::Database(db) if db.code().is_some_and(|code| RETRY_MIGRATION.contains(&code.as_ref())))
}

pub async fn migrate(pool: &PgPool, migrations: &[Migration]) -> Result<Vec<String>, DbError> {
    let mut conn = pool.acquire().await?;
    sqlx::query(VERSIONS).execute(&mut *conn).await?;
    sqlx::query(LOCK).execute(&mut *conn).await?;
    let ran = run(&mut conn, migrations).await;
    let unlocked = sqlx::query(UNLOCK).execute(&mut *conn).await;
    let ran = ran?;
    unlocked?;
    Ok(ran)
}

pub async fn apply(pool: &PgPool, migrations: &[Migration]) -> Result<usize, DbError> {
    migrate(pool, migrations).await?;
    Ok(migrations.len())
}

pub async fn applied(pool: &PgPool) -> Result<Vec<Applied>, DbError> {
    let rows: Vec<(i32, String, String)> = sqlx::query_as(KNOWN).fetch_all(pool).await?;
    Ok(rows
        .into_iter()
        .map(|(version, name, checksum)| Applied {
            version,
            name,
            checksum,
        })
        .collect())
}

async fn run(
    conn: &mut PoolConnection<Postgres>,
    migrations: &[Migration],
) -> Result<Vec<String>, DbError> {
    let known: Vec<(i32, String, String)> = sqlx::query_as(KNOWN).fetch_all(&mut **conn).await?;
    let mut last = -1;
    let mut ran = Vec::new();
    for (name, sql) in migrations {
        let number = version(name)?;
        if number <= last {
            return Err(DbError::Migration(format!(
                "{name}: версии идут строго по возрастанию"
            )));
        }
        last = number;
        let sum = checksum(sql);
        if let Some((_, _, stored)) = known.iter().find(|(seen, _, _)| *seen == number) {
            if *stored != sum {
                return Err(DbError::Migration(format!(
                    "{name}: файл изменился после применения, пиши новую миграцию"
                )));
            }
            continue;
        }
        let mut attempt = 1;
        loop {
            match once(conn, number, name, sql, &sum).await {
                Ok(()) => break,
                Err(error) if locked_out(&error) && attempt < LOCK_ATTEMPTS => {
                    tokio::time::sleep(LOCK_PAUSE * attempt).await;
                    attempt += 1;
                }
                Err(error) => return Err(named(name)(error)),
            }
        }
        ran.push(name.to_string());
    }
    Ok(ran)
}

async fn once(
    conn: &mut PgConnection,
    number: i32,
    name: &str,
    sql: &str,
    sum: &str,
) -> Result<(), sqlx::Error> {
    let timeout = LOCK_TIMEOUT.as_millis().to_string();
    if sql.starts_with(NO_TRANSACTION) {
        sqlx::query(SESSION_LOCK_TIMEOUT)
            .bind(&timeout)
            .execute(&mut *conn)
            .await?;
        for statement in statements(sql) {
            conn.execute(sqlx::raw_sql(statement)).await?;
        }
        sqlx::query(SESSION_LOCK_TIMEOUT)
            .bind("0")
            .execute(&mut *conn)
            .await?;
        sqlx::query(RECORD)
            .bind(number)
            .bind(name)
            .bind(sum)
            .execute(&mut *conn)
            .await?;
        return Ok(());
    }
    let mut tx = conn.begin().await?;
    sqlx::query(LOCAL_LOCK_TIMEOUT)
        .bind(&timeout)
        .execute(&mut *tx)
        .await?;
    tx.execute(sqlx::raw_sql(sql)).await?;
    sqlx::query(RECORD)
        .bind(number)
        .bind(name)
        .bind(sum)
        .execute(&mut *tx)
        .await?;
    tx.commit().await
}

fn statements(sql: &str) -> Vec<&str> {
    sql.split(";\n")
        .map(str::trim)
        .filter(|statement| {
            statement
                .lines()
                .any(|line| !line.trim().is_empty() && !line.trim_start().starts_with("--"))
        })
        .collect()
}
