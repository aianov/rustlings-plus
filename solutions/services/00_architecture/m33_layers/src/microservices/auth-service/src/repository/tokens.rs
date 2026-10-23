use std::collections::HashMap;
use std::fmt::Write;

use contracts::TOKEN_LEN;
use tokio::sync::Mutex;

use super::{BoxFuture, TokenRepository};
use crate::common::Account;

pub const TOKEN_BYTES: usize = TOKEN_LEN / 2;

#[derive(Default)]
pub struct MemoryTokens {
    issued: Mutex<HashMap<String, Account>>,
}

impl TokenRepository for MemoryTokens {
    fn issue(&self, account: Account) -> BoxFuture<'_, Result<String, getrandom::Error>> {
        Box::pin(async move {
            let mut raw = [0u8; TOKEN_BYTES];
            getrandom::fill(&mut raw)?;
            let mut token = String::with_capacity(TOKEN_LEN);
            for byte in raw {
                let _ = write!(token, "{byte:02x}");
            }
            self.issued.lock().await.insert(token.clone(), account);
            Ok(token)
        })
    }

    fn revoke(&self, token: String) -> BoxFuture<'_, bool> {
        Box::pin(async move { self.issued.lock().await.remove(&token).is_some() })
    }

    fn account(&self, token: String) -> BoxFuture<'_, Option<Account>> {
        Box::pin(async move { self.issued.lock().await.get(&token).cloned() })
    }
}
