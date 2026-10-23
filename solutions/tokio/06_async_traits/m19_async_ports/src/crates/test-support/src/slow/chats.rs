use std::sync::Arc;

use social_core::{BoxFuture, ChatRepo, DomainError, Message, UserId};

use super::Pace;

pub struct SlowChats {
    inner: Arc<dyn ChatRepo>,
    pace: Arc<Pace>,
}

impl SlowChats {
    pub fn new(inner: Arc<dyn ChatRepo>, pace: Arc<Pace>) -> SlowChats {
        SlowChats { inner, pace }
    }
}

impl ChatRepo for SlowChats {
    fn send(
        &self,
        from: UserId,
        to: UserId,
        text: String,
    ) -> BoxFuture<'_, Result<Message, DomainError>> {
        Box::pin(async move {
            self.pace.wait().await;
            self.inner.send(from, to, text).await
        })
    }

    fn history(
        &self,
        user: UserId,
        peer: UserId,
        limit: usize,
    ) -> BoxFuture<'_, Result<Vec<Message>, DomainError>> {
        Box::pin(async move {
            self.pace.wait().await;
            self.inner.history(user, peer, limit).await
        })
    }
}
