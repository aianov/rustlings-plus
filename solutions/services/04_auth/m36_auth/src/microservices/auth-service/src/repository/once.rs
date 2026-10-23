use std::collections::HashMap;

use tokio::sync::Mutex;

use super::{BoxFuture, OneTime, OneTimeRepository, Purpose};

#[derive(Default)]
pub struct MemoryOneTime {
    tokens: Mutex<HashMap<String, OneTime>>,
}

impl OneTimeRepository for MemoryOneTime {
    fn issue(&self, hash: String, token: OneTime) -> BoxFuture<'_, ()> {
        Box::pin(async move {
            let mut tokens = self.tokens.lock().await;
            tokens.retain(|_, old| !(old.purpose == token.purpose && old.user_id == token.user_id));
            tokens.insert(hash, token);
        })
    }

    fn take(&self, purpose: Purpose, hash: String, now: u64) -> BoxFuture<'_, Option<u64>> {
        Box::pin(async move {
            let mut tokens = self.tokens.lock().await;
            let found = tokens.get(&hash).copied()?;
            if found.purpose != purpose {
                return None;
            }
            tokens.remove(&hash);
            (now < found.expires).then_some(found.user_id)
        })
    }
}
