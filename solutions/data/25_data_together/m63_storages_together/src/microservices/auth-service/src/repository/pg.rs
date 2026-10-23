use std::io;

use db_engine::{DbError, Migration};
use service_runtime::StorageError;
use sqlx::PgPool;

use super::{AccountRepository, BoxFuture, OneTime, OneTimeRepository, Purpose, RepoError};
use crate::common::Account;

pub const MIGRATIONS: [Migration; 1] = [(
    "0001_init.sql",
    include_str!("../../migrations/0001_init.sql"),
)];

pub const ACCOUNT_TAKEN: &str = "accounts_username_key";

const ACCOUNT_ID_TAKEN: &str = "accounts_pkey";

const INSERT_ACCOUNT: &str = "INSERT INTO accounts (user_id, username, email, email_verified, password, role, frozen, deleted) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)";
const SELECT_BY_NAME: &str = "SELECT user_id, username, email, email_verified, password, role, frozen, deleted FROM accounts WHERE lower(username) = lower($1) AND NOT deleted";
const SELECT_BY_ID: &str = "SELECT user_id, username, email, email_verified, password, role, frozen, deleted FROM accounts WHERE user_id = $1 AND NOT deleted";
const UPDATE_ACCOUNT: &str = "UPDATE accounts SET username = $2, email = $3, email_verified = $4, password = $5, role = $6, frozen = $7, deleted = $8 WHERE user_id = $1";
const CLEAR_ONE_TIME: &str = "DELETE FROM one_time_tokens WHERE purpose = $1 AND user_id = $2";
const INSERT_ONE_TIME: &str = "INSERT INTO one_time_tokens (hash, purpose, user_id, expires) VALUES ($1, $2, $3, $4) ON CONFLICT (hash) DO UPDATE SET purpose = EXCLUDED.purpose, user_id = EXCLUDED.user_id, expires = EXCLUDED.expires";
const TAKE_ONE_TIME: &str =
    "DELETE FROM one_time_tokens WHERE hash = $1 AND purpose = $2 RETURNING user_id, expires";

#[derive(sqlx::FromRow)]
struct AccountRow {
    user_id: i64,
    username: String,
    email: Option<String>,
    email_verified: bool,
    password: Option<String>,
    role: String,
    frozen: bool,
    deleted: bool,
}

pub(super) fn storage(error: impl std::fmt::Display) -> StorageError {
    StorageError::Io(io::Error::other(error.to_string()))
}

pub(super) fn big(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

pub(super) fn small(value: i64) -> u64 {
    u64::try_from(value).unwrap_or(0)
}

fn account(row: AccountRow) -> Account {
    Account {
        user_id: small(row.user_id),
        username: row.username,
        email: row.email,
        email_verified: row.email_verified,
        password: row.password,
        role: row.role,
        frozen: row.frozen,
        deleted: row.deleted,
    }
}

fn purpose(purpose: Purpose) -> String {
    let name = match purpose {
        Purpose::ConfirmEmail => "confirm_email",
        Purpose::ResetPassword => "reset_password",
    };
    name.to_string()
}

pub struct PgAccounts {
    pub(super) pool: PgPool,
    pub(super) leeway: u64,
}

impl PgAccounts {
    pub fn new(pool: PgPool, leeway: u64) -> PgAccounts {
        PgAccounts { pool, leeway }
    }

    async fn write(&self, query: &str, account: &Account) -> Result<u64, DbError> {
        let done = sqlx::query(query)
            .bind(big(account.user_id))
            .bind(&account.username)
            .bind(&account.email)
            .bind(account.email_verified)
            .bind(&account.password)
            .bind(&account.role)
            .bind(account.frozen)
            .bind(account.deleted)
            .execute(&self.pool)
            .await?;
        Ok(done.rows_affected())
    }

    async fn by_name(&self, username: &str) -> Option<Account> {
        let row: Option<AccountRow> = sqlx::query_as(SELECT_BY_NAME)
            .bind(username)
            .fetch_optional(&self.pool)
            .await
            .ok()?;
        row.map(account)
    }

    async fn by_id(&self, user_id: u64) -> Option<Account> {
        let row: Option<AccountRow> = sqlx::query_as(SELECT_BY_ID)
            .bind(big(user_id))
            .fetch_optional(&self.pool)
            .await
            .ok()?;
        row.map(account)
    }
}

impl AccountRepository for PgAccounts {
    fn create_if_absent(&self, created: Account) -> BoxFuture<'_, Result<Account, RepoError>> {
        Box::pin(async move {
            match self.write(INSERT_ACCOUNT, &created).await {
                Ok(..) => Ok(created),
                Err(error)
                    if error.is_unique(ACCOUNT_TAKEN) || error.is_unique(ACCOUNT_ID_TAKEN) =>
                {
                    Err(RepoError::Conflict)
                }
                Err(error) => Err(RepoError::Storage(storage(error))),
            }
        })
    }

    fn find(&self, username: String) -> BoxFuture<'_, Option<Account>> {
        Box::pin(async move { self.by_name(&username).await })
    }

    fn get(&self, user_id: u64) -> BoxFuture<'_, Option<Account>> {
        Box::pin(async move { self.by_id(user_id).await })
    }

    fn update(&self, changed: Account) -> BoxFuture<'_, Result<(), StorageError>> {
        Box::pin(async move {
            self.write(UPDATE_ACCOUNT, &changed)
                .await
                .map(|_| ())
                .map_err(storage)
        })
    }
}

impl OneTimeRepository for PgAccounts {
    fn issue(&self, hash: String, token: OneTime) -> BoxFuture<'_, ()> {
        Box::pin(async move {
            let _ = sqlx::query(CLEAR_ONE_TIME)
                .bind(purpose(token.purpose))
                .bind(big(token.user_id))
                .execute(&self.pool)
                .await;
            let _ = sqlx::query(INSERT_ONE_TIME)
                .bind(hash)
                .bind(purpose(token.purpose))
                .bind(big(token.user_id))
                .bind(big(token.expires))
                .execute(&self.pool)
                .await;
        })
    }

    fn take(&self, wanted: Purpose, hash: String, now: u64) -> BoxFuture<'_, Option<u64>> {
        Box::pin(async move {
            let row: Option<(i64, i64)> = sqlx::query_as(TAKE_ONE_TIME)
                .bind(hash)
                .bind(purpose(wanted))
                .fetch_optional(&self.pool)
                .await
                .ok()
                .flatten();
            let (user, expires) = row?;
            (now < small(expires)).then_some(small(user))
        })
    }
}
