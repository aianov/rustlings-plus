use std::sync::Arc;

use contracts::{DomainEvent, Fact, FactError, UserId};

use crate::common::{Change, Consumed, FeedItem};
use crate::repository::{FeedRepository, Snapshot};

pub struct FeedService {
    feed: Arc<dyn FeedRepository>,
}

fn change_of(fact: Fact, at: u64) -> Change {
    match fact {
        Fact::Registered(fact) => Change::Registered {
            user: fact.user_id,
            username: fact.username,
        },
        Fact::ProfileChanged(fact) => Change::Renamed {
            user: fact.user_id,
            display_name: fact.display_name,
        },
        Fact::Followed(fact) => Change::Followed {
            follower: fact.follower_id,
            followee: fact.followee_id,
        },
        Fact::Muted(fact) => Change::Muted {
            user: fact.user_id,
            muted: fact.muted_id,
        },
        Fact::PostCreated(fact) => Change::Posted {
            post: fact.post_id,
            author: fact.author_id,
            author_name: fact.author_name,
            text: fact.text,
            tags: fact.tags,
            links: fact.links,
            at,
        },
        Fact::PostLiked(fact) => Change::Liked { post: fact.post_id },
        Fact::PostEdited(fact) => Change::Edited {
            post: fact.post_id,
            text: fact.text,
        },
        Fact::PostDeleted(fact) => Change::Deleted { post: fact.post_id },
        Fact::PostCommented(_) | Fact::MessageSent(_) => Change::Nothing,
    }
}

impl FeedService {
    pub fn new(feed: Arc<dyn FeedRepository>) -> FeedService {
        FeedService { feed }
    }

    pub async fn consume(&self, event: &DomainEvent) -> Result<Consumed, FactError> {
        let (change, broken) = match event.fact() {
            Ok(Some(fact)) => (change_of(fact, event.occurred_at), None),
            Ok(None) => (Change::Nothing, None),
            Err(error) => (Change::Nothing, Some(error)),
        };
        let aggregate = event.aggregate_id.clone();
        let consumed = self.feed.apply(aggregate, event.version, change).await;
        match broken {
            Some(error) => Err(error),
            None => Ok(consumed),
        }
    }

    pub async fn page(&self, viewer: UserId, page: usize, size: usize) -> Vec<FeedItem> {
        let skip = page.saturating_sub(1).saturating_mul(size);
        self.feed.page(viewer, skip, size).await
    }

    pub async fn snapshot(&self) -> Snapshot {
        self.feed.snapshot().await
    }
}
