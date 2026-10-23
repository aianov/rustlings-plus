use std::collections::HashMap;

use contracts::UserId;
use tokio::sync::Mutex;

use super::{AvatarRepository, BoxFuture};
use crate::common::Avatar;

#[derive(Default)]
pub struct MemoryAvatars {
    stored: Mutex<HashMap<UserId, Avatar>>,
}

impl AvatarRepository for MemoryAvatars {
    fn put(&self, owner: UserId, avatar: Avatar) -> BoxFuture<'_, bool> {
        Box::pin(async move { self.stored.lock().await.insert(owner, avatar).is_none() })
    }

    fn get(&self, owner: UserId) -> BoxFuture<'_, Option<Avatar>> {
        Box::pin(async move { self.stored.lock().await.get(&owner).cloned() })
    }

    fn remove(&self, owner: UserId) -> BoxFuture<'_, bool> {
        Box::pin(async move { self.stored.lock().await.remove(&owner).is_some() })
    }
}
