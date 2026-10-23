mod accounts;
mod tokens;

use std::future::Future;
use std::pin::Pin;

use journal::JournalError;

use crate::common::Account;

pub use accounts::MemoryAccounts;
pub use tokens::{MemoryTokens, TOKEN_BYTES};

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
}

pub trait TokenRepository: Send + Sync {
    fn issue(&self, account: Account) -> BoxFuture<'_, Result<String, getrandom::Error>>;
    fn revoke(&self, token: String) -> BoxFuture<'_, bool>;
    fn account(&self, token: String) -> BoxFuture<'_, Option<Account>>;
}
