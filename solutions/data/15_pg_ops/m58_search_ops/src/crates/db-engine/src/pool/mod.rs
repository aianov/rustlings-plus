use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{ConnectOptions, Connection, Executor, PgPool};

use crate::config::{DbConfig, good_schema};
use crate::error::DbError;
use crate::migrate::{Migration, migrate};

fn options(config: &DbConfig) -> Result<PgConnectOptions, DbError> {
    config
        .url
        .expose()
        .parse::<PgConnectOptions>()
        .map(|options| options.disable_statement_logging())
        .map_err(|error| DbError::Unavailable(error.to_string()))
}

pub async fn prepare(config: &DbConfig, migrations: &[Migration]) -> Result<PgPool, DbError> {
    if config.migrate_url.is_none() {
        let pool = connect(config).await?;
        migrate(&pool, migrations).await?;
        return Ok(pool);
    }
    let owner = connect_owner(config).await?;
    let ran = migrate(&owner, migrations).await;
    owner.close().await;
    ran?;
    connect(config).await
}

pub async fn connect_owner(config: &DbConfig) -> Result<PgPool, DbError> {
    match &config.migrate_url {
        Some(url) => {
            let owner = DbConfig {
                url: url.clone(),
                migrate_url: None,
                ..config.clone()
            };
            connect(&owner).await
        }
        None => connect(config).await,
    }
}

pub async fn connect(config: &DbConfig) -> Result<PgPool, DbError> {
    if !good_schema(&config.schema) {
        return Err(DbError::BadSchema(config.schema.clone()));
    }
    let base = options(config)?;
    let mut first = base.connect().await?;
    let known: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pg_namespace WHERE nspname = $1)")
            .bind(&config.schema)
            .fetch_one(&mut first)
            .await?;
    if !known {
        first
            .execute(sqlx::raw_sql(&format!(
                "CREATE SCHEMA IF NOT EXISTS {}",
                config.schema
            )))
            .await?;
    }
    first.close().await?;
    let path = format!("SET search_path TO {}, public", config.schema);
    let pool = PgPoolOptions::new()
        .max_connections(config.max_connections)
        .acquire_timeout(config.acquire_timeout)
        .after_connect(move |conn, _| {
            let path = path.clone();
            Box::pin(async move {
                conn.execute(sqlx::raw_sql(&path)).await?;
                Ok(())
            })
        })
        .connect_with(base)
        .await?;
    Ok(pool)
}

pub async fn drop_schema(config: &DbConfig) -> Result<(), DbError> {
    if !good_schema(&config.schema) {
        return Err(DbError::BadSchema(config.schema.clone()));
    }
    let mut conn = options(config)?.connect().await?;
    conn.execute(sqlx::raw_sql(&format!(
        "DROP SCHEMA IF EXISTS {} CASCADE",
        config.schema
    )))
    .await?;
    conn.close().await?;
    Ok(())
}
