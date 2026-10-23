mod accounts;
mod once;
mod sessions;

use std::future::Future;
use std::pin::Pin;

use journal::JournalError;

use crate::common::Account;

pub use accounts::MemoryAccounts;
pub use once::MemoryOneTime;
pub use sessions::MemorySessions;

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

#[derive(Debug)]
pub enum RepoError {
    Conflict,
    Storage(JournalError),
}

pub trait AccountRepository: Send + Sync {
    fn create_if_absent(&self, account: Account) -> BoxFuture<'_, Result<Account, RepoError>>;
    fn find(&self, username: String) -> BoxFuture<'_, Option<Account>>;
    fn get(&self, user_id: u64) -> BoxFuture<'_, Option<Account>>;
    fn update(&self, account: Account) -> BoxFuture<'_, Result<(), JournalError>>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRow {
    pub family: u64,
    pub user_id: u64,
    pub used: bool,
    pub expires: u64,
    pub access_jti: String,
    pub access_exp: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Opening {
    pub refresh_hash: String,
    pub access_jti: String,
    pub access_exp: u64,
    pub expires: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshError {
    Unknown,
    Reused,
    Expired,
    Revoked,
}

pub trait SessionRepository: Send + Sync {
    fn open(&self, user_id: u64, opening: Opening) -> BoxFuture<'_, SessionRow>;
    fn rotate(
        &self,
        old_hash: String,
        now: u64,
        opening: Opening,
    ) -> BoxFuture<'_, Result<SessionRow, RefreshError>>;
    fn family_of(&self, jti: String) -> BoxFuture<'_, Option<SessionRow>>;
    fn revoke_family(&self, family: u64) -> BoxFuture<'_, ()>;
    fn revoke_user(&self, user_id: u64, keep: Option<u64>) -> BoxFuture<'_, Vec<u64>>;
    fn row(&self, refresh_hash: String) -> BoxFuture<'_, Option<SessionRow>>;
    fn deny(&self, jti: String, until: u64) -> BoxFuture<'_, ()>;
    fn is_denied(&self, jti: String) -> BoxFuture<'_, bool>;
    fn purge(&self, now: u64) -> BoxFuture<'_, usize>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Purpose {
    ConfirmEmail,
    ResetPassword,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OneTime {
    pub purpose: Purpose,
    pub user_id: u64,
    pub expires: u64,
}

pub trait OneTimeRepository: Send + Sync {
    fn issue(&self, hash: String, token: OneTime) -> BoxFuture<'_, ()>;
    fn take(&self, purpose: Purpose, hash: String, now: u64) -> BoxFuture<'_, Option<u64>>;
}
