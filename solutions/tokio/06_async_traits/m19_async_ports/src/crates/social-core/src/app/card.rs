use std::sync::Arc;

use tokio::time::timeout;

use super::{App, AppError, Op};
use crate::domain::{DomainError, PostId, User, UserId};
use crate::ports::{AuthorStats, FollowCounts};

pub const CARD_POSTS: usize = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Card {
    pub id: UserId,
    pub name: String,
    pub bio: String,
    pub followers: usize,
    pub following: usize,
    pub posts: usize,
    pub likes: usize,
    pub recent: Vec<PostId>,
    pub followed: bool,
}

impl App {
    pub async fn card(&self, viewer: UserId, whose: UserId) -> Result<Card, AppError> {
        let after = self.inner.config.op_timeout;
        let parts = async {
            tokio::try_join!(
                self.card_user(whose),
                self.card_counts(whose),
                self.card_recent(viewer, whose),
                self.card_followed(viewer, whose),
            )
        };
        let (user, (follows, stats), recent, followed) =
            timeout(after, parts)
                .await
                .map_err(|_| AppError::Timeout {
                    op: Op::Card,
                    after,
                })??;
        Ok(Card {
            id: user.id(),
            name: user.name().to_string(),
            bio: user.bio().to_string(),
            followers: follows.followers,
            following: follows.following,
            posts: stats.posts,
            likes: stats.likes,
            recent,
            followed,
        })
    }

    async fn card_user(&self, id: UserId) -> Result<Arc<User>, AppError> {
        self.profile(id)
            .await
            .ok_or(AppError::Domain(DomainError::UnknownUser(id)))
    }

    async fn card_counts(&self, id: UserId) -> Result<(FollowCounts, AuthorStats), AppError> {
        Ok(tokio::join!(self.follow_counts(id), self.author_stats(id)))
    }

    async fn card_recent(&self, viewer: UserId, whose: UserId) -> Result<Vec<PostId>, AppError> {
        let page = self.author_page(viewer, whose, None, CARD_POSTS).await?;
        Ok(page.posts.iter().map(|post| post.id).collect())
    }

    async fn card_followed(&self, viewer: UserId, whose: UserId) -> Result<bool, AppError> {
        Ok(self.is_following(viewer, whose).await)
    }
}
