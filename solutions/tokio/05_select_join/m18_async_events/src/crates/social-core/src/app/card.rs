use std::sync::Arc;

use tokio::time::timeout;

use super::{App, AppError, Op};
use crate::domain::{DomainError, Post, PostId, User, UserId};
use crate::ports::{PostStore, UserStore};

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

struct Counts {
    followers: usize,
    following: usize,
    posts: usize,
    likes: usize,
}

impl<U, P> App<U, P>
where
    U: UserStore + Send + 'static,
    P: PostStore + Send + 'static,
{
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
        let (user, counts, recent, followed) =
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
            followers: counts.followers,
            following: counts.following,
            posts: counts.posts,
            likes: counts.likes,
            recent,
            followed,
        })
    }

    async fn card_user(&self, id: UserId) -> Result<Arc<User>, AppError> {
        self.profile(id)
            .ok_or(AppError::Domain(DomainError::UnknownUser(id)))
    }

    async fn card_counts(&self, id: UserId) -> Result<Counts, AppError> {
        Ok(self.read(|network| Counts {
            followers: network.followers_count(id),
            following: network.following_count(id),
            posts: network.post_count_of(id),
            likes: network.likes_received(id),
        }))
    }

    async fn card_recent(&self, viewer: UserId, whose: UserId) -> Result<Vec<PostId>, AppError> {
        let page = self.read(|network| {
            network
                .author_page(viewer, whose, None, CARD_POSTS)
                .map(|page| page.posts.iter().copied().map(Post::id).collect())
        });
        Ok(page?)
    }

    async fn card_followed(&self, viewer: UserId, whose: UserId) -> Result<bool, AppError> {
        Ok(self.read(|network| network.is_following(viewer, whose)))
    }
}
