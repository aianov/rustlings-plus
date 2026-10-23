use std::collections::HashMap;
use std::fmt::Write;
use std::sync::{Mutex, MutexGuard, PoisonError};

use contracts::TOKEN_LEN;

use crate::accounts::Account;

pub const TOKEN_BYTES: usize = TOKEN_LEN / 2;

#[derive(Default)]
pub struct Tokens {
    issued: Mutex<HashMap<String, Account>>,
}

impl Tokens {
    fn lock(&self) -> MutexGuard<'_, HashMap<String, Account>> {
        self.issued.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn issue(&self, account: Account) -> Result<String, getrandom::Error> {
        let mut raw = [0u8; TOKEN_BYTES];
        getrandom::fill(&mut raw)?;
        let mut token = String::with_capacity(TOKEN_LEN);
        for byte in raw {
            let _ = write!(token, "{byte:02x}");
        }
        self.lock().insert(token.clone(), account);
        Ok(token)
    }

    pub fn revoke(&self, token: &str) -> bool {
        self.lock().remove(token).is_some()
    }

    pub fn account(&self, token: &str) -> Option<Account> {
        self.lock().get(token).cloned()
    }

    pub fn len(&self) -> usize {
        self.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
