use std::collections::HashMap;

use contracts::UserId;
use tokio::sync::Mutex;

use super::{AvatarRepository, BoxFuture};
use crate::common::Avatar;

#[derive(Default)]
struct State {
    links: HashMap<UserId, Avatar>,
    pending: HashMap<String, u64>,
}

#[derive(Default)]
pub struct MemoryAvatars {
    state: Mutex<State>,
}

impl AvatarRepository for MemoryAvatars {
    fn link(&self, owner: UserId, avatar: Avatar) -> BoxFuture<'_, Option<Avatar>> {
        Box::pin(async move { self.state.lock().await.links.insert(owner, avatar) })
    }

    fn get(&self, owner: UserId) -> BoxFuture<'_, Option<Avatar>> {
        Box::pin(async move { self.state.lock().await.links.get(&owner).cloned() })
    }

    fn remove(&self, owner: UserId) -> BoxFuture<'_, Option<Avatar>> {
        Box::pin(async move { self.state.lock().await.links.remove(&owner) })
    }

    fn referenced(&self, key: String) -> BoxFuture<'_, bool> {
        Box::pin(async move {
            let state = self.state.lock().await;
            state.links.values().any(|avatar| avatar.key == key)
        })
    }

    fn pending(&self, key: String, at: u64) -> BoxFuture<'_, ()> {
        Box::pin(async move {
            self.state.lock().await.pending.insert(key, at);
        })
    }

    fn settle(&self, key: String) -> BoxFuture<'_, ()> {
        Box::pin(async move {
            self.state.lock().await.pending.remove(&key);
        })
    }

    fn stale(&self, before: u64) -> BoxFuture<'_, Vec<String>> {
        Box::pin(async move {
            let state = self.state.lock().await;
            let mut keys: Vec<String> = state
                .pending
                .iter()
                .filter(|(_, at)| **at < before)
                .map(|(key, _)| key.clone())
                .collect();
            keys.sort();
            keys
        })
    }
}
