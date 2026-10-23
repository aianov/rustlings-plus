mod directory;
mod error;

use std::sync::Arc;

use contracts::Username;

use crate::common::Account;
use crate::repository::{AccountRepository, RepoError, TokenRepository};

pub use directory::{ProfileDirectory, ProfileError};
pub use error::AuthError;

pub struct AuthService {
    accounts: Arc<dyn AccountRepository>,
    tokens: Arc<dyn TokenRepository>,
}

impl AuthService {
    pub fn new(
        accounts: Arc<dyn AccountRepository>,
        tokens: Arc<dyn TokenRepository>,
    ) -> AuthService {
        AuthService { accounts, tokens }
    }

    pub async fn register(
        &self,
        profiles: &dyn ProfileDirectory,
        name: &Username,
    ) -> Result<Account, AuthError> {
        let account = profiles
            .create(name.as_ref().to_string())
            .await
            .map_err(AuthError::Profile)?;
        match self.accounts.create_if_absent(account).await {
            Ok(account) => Ok(account),
            Err(RepoError::Conflict) => Err(AuthError::Duplicate(name.as_ref().to_string())),
            Err(RepoError::Storage(error)) => Err(AuthError::Storage(error)),
        }
    }

    pub async fn login(&self, name: &Username) -> Result<(String, Account), AuthError> {
        let found = self.accounts.find(name.as_ref().to_string()).await;
        let account = found.ok_or_else(|| AuthError::UnknownName(name.as_ref().to_string()))?;
        let token = self
            .tokens
            .issue(account.clone())
            .await
            .map_err(AuthError::Random)?;
        Ok((token, account))
    }

    pub async fn logout(&self, token: &str) -> bool {
        self.tokens.revoke(token.to_string()).await
    }

    pub async fn validate(&self, token: &str) -> Option<Account> {
        self.tokens.account(token.to_string()).await
    }

    pub async fn account(&self, user_id: u64) -> Option<Account> {
        self.accounts.get(user_id).await
    }
}
