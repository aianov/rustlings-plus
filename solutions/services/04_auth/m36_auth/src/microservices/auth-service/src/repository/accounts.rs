use std::collections::HashMap;

use journal::{Journal, JournalError};
use tokio::sync::Mutex;

use super::{AccountRepository, BoxFuture, RepoError};
use crate::common::Account;

#[derive(Default)]
struct Known {
    by_id: HashMap<u64, Account>,
    by_name: HashMap<String, u64>,
}

impl Known {
    fn insert(&mut self, account: Account) {
        self.by_name
            .insert(account.username.to_lowercase(), account.user_id);
        self.by_id.insert(account.user_id, account);
    }
}

pub struct MemoryAccounts {
    known: Mutex<Known>,
    journal: Journal<Account>,
}

impl MemoryAccounts {
    pub fn new() -> MemoryAccounts {
        MemoryAccounts::restore(Journal::memory(), Vec::new())
    }

    pub fn restore(journal: Journal<Account>, accounts: Vec<Account>) -> MemoryAccounts {
        let mut known = Known::default();
        for account in accounts {
            known.insert(account);
        }
        MemoryAccounts {
            known: Mutex::new(known),
            journal,
        }
    }
}

impl Default for MemoryAccounts {
    fn default() -> MemoryAccounts {
        MemoryAccounts::new()
    }
}

impl AccountRepository for MemoryAccounts {
    fn create_if_absent(&self, account: Account) -> BoxFuture<'_, Result<Account, RepoError>> {
        Box::pin(async move {
            let mut known = self.known.lock().await;
            let name = account.username.to_lowercase();
            if known.by_name.contains_key(&name) || known.by_id.contains_key(&account.user_id) {
                return Err(RepoError::Conflict);
            }
            self.journal
                .append(&account)
                .await
                .map_err(RepoError::Storage)?;
            known.insert(account.clone());
            Ok(account)
        })
    }

    fn find(&self, username: String) -> BoxFuture<'_, Option<Account>> {
        Box::pin(async move {
            let known = self.known.lock().await;
            let id = known.by_name.get(&username.to_lowercase())?;
            known.by_id.get(id).cloned()
        })
    }

    fn get(&self, user_id: u64) -> BoxFuture<'_, Option<Account>> {
        Box::pin(async move { self.known.lock().await.by_id.get(&user_id).cloned() })
    }

    fn update(&self, account: Account) -> BoxFuture<'_, Result<(), JournalError>> {
        Box::pin(async move {
            let mut known = self.known.lock().await;
            self.journal.append(&account).await?;
            known.insert(account);
            Ok(())
        })
    }
}
