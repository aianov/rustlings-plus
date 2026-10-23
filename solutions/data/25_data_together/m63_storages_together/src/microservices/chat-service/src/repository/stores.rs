use std::fmt;
use std::sync::Arc;

use super::{ChatRepository, MemoryChats, OutboxRepository, ScyllaChats};

#[derive(Clone)]
pub struct Stores {
    pub chats: Arc<dyn ChatRepository>,
    pub outbox: Arc<dyn OutboxRepository>,
    pub scylla: Option<Arc<ScyllaChats>>,
}

impl Stores {
    pub fn memory() -> Stores {
        let chats = Arc::new(MemoryChats::new());
        Stores {
            chats: chats.clone(),
            outbox: chats,
            scylla: None,
        }
    }

    pub fn scylla(chats: Arc<ScyllaChats>) -> Stores {
        Stores {
            chats: chats.clone(),
            outbox: chats.clone(),
            scylla: Some(chats),
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
        std::ptr::addr_eq(Arc::as_ptr(&self.outbox), Arc::as_ptr(&other.outbox))
    }
}

impl Eq for Stores {}
