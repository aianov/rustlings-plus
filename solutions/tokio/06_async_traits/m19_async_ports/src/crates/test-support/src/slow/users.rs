use std::sync::Arc;

use social_core::{BoxFuture, DomainError, FollowCounts, User, UserId, UserRepo};

use super::Pace;

pub struct SlowUsers {
    inner: Arc<dyn UserRepo>,
    pace: Arc<Pace>,
}

impl SlowUsers {
    pub fn new(inner: Arc<dyn UserRepo>, pace: Arc<Pace>) -> SlowUsers {
        SlowUsers { inner, pace }
    }
}

impl UserRepo for SlowUsers {
    fn register(&self, name: String) -> BoxFuture<'_, Result<UserId, DomainError>> {
        Box::pin(async move {
            self.pace.wait().await;
            self.inner.register(name).await
        })
    }

    fn user(&self, id: UserId) -> BoxFuture<'_, Option<User>> {
        Box::pin(async move {
            self.pace.wait().await;
            self.inner.user(id).await
        })
    }

    fn find(&self, name: String) -> BoxFuture<'_, Option<User>> {
        Box::pin(async move {
            self.pace.wait().await;
            self.inner.find(name).await
        })
    }

    fn follow(&self, who: UserId, whom: UserId) -> BoxFuture<'_, Result<bool, DomainError>> {
        Box::pin(async move {
            self.pace.wait().await;
            self.inner.follow(who, whom).await
        })
    }

    fn mute(&self, who: UserId, whom: UserId) -> BoxFuture<'_, Result<bool, DomainError>> {
        Box::pin(async move {
            self.pace.wait().await;
            self.inner.mute(who, whom).await
        })
    }

    fn is_following(&self, who: UserId, whom: UserId) -> BoxFuture<'_, bool> {
        Box::pin(async move {
            self.pace.wait().await;
            self.inner.is_following(who, whom).await
        })
    }

    fn follow_counts(&self, id: UserId) -> BoxFuture<'_, FollowCounts> {
        Box::pin(async move {
            self.pace.wait().await;
            self.inner.follow_counts(id).await
        })
    }

    fn count(&self) -> BoxFuture<'_, usize> {
        Box::pin(async move {
            self.pace.wait().await;
            self.inner.count().await
        })
    }
}
