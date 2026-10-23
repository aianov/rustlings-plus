use std::fmt;
use std::sync::Arc;

use super::{
    AccountRepository, MemoryAccounts, MemoryOneTime, MemorySessions, OneTimeRepository,
    PgAccounts, SessionRepository,
};

#[derive(Clone)]
pub struct Stores {
    pub accounts: Arc<dyn AccountRepository>,
    pub sessions: Arc<dyn SessionRepository>,
    pub once: Arc<dyn OneTimeRepository>,
}

impl Stores {
    pub fn memory(leeway: u64) -> Stores {
        Stores {
            accounts: Arc::new(MemoryAccounts::new()),
            sessions: Arc::new(MemorySessions::new(leeway)),
            once: Arc::new(MemoryOneTime::default()),
        }
    }

    pub fn postgres(accounts: Arc<PgAccounts>) -> Stores {
        Stores {
            accounts: accounts.clone(),
            sessions: accounts.clone(),
            once: accounts,
        }
    }
}

impl fmt::Debug for Stores {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Stores")
    }
}

impl PartialEq for Stores {
    fn eq(&self, other: &Stores) -> bool {
        std::ptr::addr_eq(Arc::as_ptr(&self.accounts), Arc::as_ptr(&other.accounts))
    }
}

impl Eq for Stores {}
