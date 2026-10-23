use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use sqlx::{PgConnection, PgPool};

use crate::error::DbError;

pub const RETRY_CODES: [&str; 2] = ["40001", "40P01"];

pub type TxResult<T> = Result<T, sqlx::Error>;

pub type TxFuture<'c, T> = Pin<Box<dyn Future<Output = TxResult<T>> + Send + 'c>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TxPolicy {
    pub attempts: u32,
    pub statement_timeout: Duration,
}

impl Default for TxPolicy {
    fn default() -> TxPolicy {
        TxPolicy {
            attempts: 3,
            statement_timeout: Duration::from_secs(2),
        }
    }
}

pub fn retryable(error: &sqlx::Error) -> bool {
    match error {
        sqlx::Error::Database(db) => db
            .code()
            .is_some_and(|code| RETRY_CODES.contains(&code.as_ref())),
        _ => false,
    }
}

pub async fn transact<T, F>(pool: &PgPool, policy: TxPolicy, mut work: F) -> Result<T, DbError>
where
    T: Send,
    F: for<'c> FnMut(&'c mut PgConnection) -> TxFuture<'c, T> + Send,
{
    let attempts = policy.attempts.max(1);
    let timeout = policy.statement_timeout.as_millis().to_string();
    let mut attempt = 1;
    loop {
        let mut tx = pool.begin().await?;
        sqlx::query("SELECT set_config('statement_timeout', $1, true)")
            .bind(&timeout)
            .execute(&mut *tx)
            .await?;
        let done = match work(&mut tx).await {
            Ok(value) => tx.commit().await.map(|()| value),
            Err(error) => {
                let _ = tx.rollback().await;
                Err(error)
            }
        };
        match done {
            Ok(value) => return Ok(value),
            Err(error) if retryable(&error) && attempt < attempts => attempt += 1,
            Err(error) if retryable(&error) => {
                return Err(DbError::Conflict(format!(
                    "{attempts} попытки транзакции не прошли: {error}"
                )));
            }
            Err(error) => return Err(DbError::from(error)),
        }
    }
}
