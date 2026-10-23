use std::fmt;
use std::sync::Arc;

use super::{
    AvatarRepository, LifecycleRepository, MemoryAvatars, MemoryUsers, OutboxRepository, PgUsers,
    ProfileExtras, SocialQueries, UserRepository,
};

#[derive(Clone)]
pub struct Stores {
    pub users: Arc<dyn UserRepository>,
    pub avatars: Arc<dyn AvatarRepository>,
    pub lifecycle: Arc<dyn LifecycleRepository>,
    pub queries: Arc<dyn SocialQueries>,
    pub extras: Option<Arc<dyn ProfileExtras>>,
    pub outbox: Arc<dyn OutboxRepository>,
}

impl Stores {
    pub fn memory() -> Stores {
        let users = Arc::new(MemoryUsers::new());
        Stores {
            users: users.clone(),
            avatars: Arc::new(MemoryAvatars::default()),
            lifecycle: users.clone(),
            queries: users.clone(),
            extras: None,
            outbox: users,
        }
    }

    pub fn postgres(users: Arc<PgUsers>) -> Stores {
        Stores {
            users: users.clone(),
            avatars: users.clone(),
            lifecycle: users.clone(),
            queries: users.clone(),
            extras: Some(users.clone()),
            outbox: users,
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
