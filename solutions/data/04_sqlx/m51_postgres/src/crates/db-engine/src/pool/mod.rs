use std::time::Duration;

use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{ConnectOptions, Connection, Executor, PgPool};

use crate::config::{DbConfig, good_schema};
use crate::error::DbError;

fn options(config: &DbConfig) -> Result<PgConnectOptions, DbError> {
    config
        .url
        .expose()
        .parse::<PgConnectOptions>()
        .map(|options| options.disable_statement_logging())
        .map_err(|error| DbError::Unavailable(error.to_string()))
}

pub async fn connect(config: &DbConfig) -> Result<PgPool, DbError> {
    if !good_schema(&config.schema) {
        return Err(DbError::BadSchema(config.schema.clone()));
    }
    let base = options(config)?;
    let mut first = base.connect().await?;
    first
        .execute(sqlx::raw_sql(&format!(
            "CREATE SCHEMA IF NOT EXISTS {}",
            config.schema
        )))
        .await?;
    first.close().await?;
    let path = format!("SET search_path TO {}", config.schema);
    let pool = PgPoolOptions::new()
        .max_connections(config.max_connections)
        .acquire_timeout(Duration::from_secs(5))
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
