use std::collections::HashSet;
use std::sync::Arc;

use contracts::{Permission, PostId, Role, UserId};

use super::{PostError, PostService};
use crate::common::{Post, now_micros};
use crate::repository::PostQueries;

pub const MAX_IDS: usize = 100;

pub const MAX_DAYS: u32 = 90;

pub struct Totals {
    pub posts: u64,
    pub likes: u64,
}

pub struct Day {
    pub day: String,
    pub posts: u64,
    pub likes: u64,
}

impl PostService {
    pub fn with_queries(self, queries: Arc<dyn PostQueries>) -> PostService {
        PostService {
            queries: Some(queries),
            ..self
        }
    }

    fn queries(&self) -> Result<&dyn PostQueries, PostError> {
        self.queries.as_deref().ok_or(PostError::NoQueries)
    }

    pub async fn hydrate(
        &self,
        viewer: Option<UserId>,
        ids: Vec<PostId>,
        hidden: Vec<UserId>,
    ) -> Result<Vec<(Post, bool)>, PostError> {
        if ids.len() > MAX_IDS {
            return Err(PostError::TooMany(ids.len()));
        }
        let mut seen = HashSet::new();
        let ids: Vec<PostId> = ids.into_iter().filter(|id| seen.insert(*id)).collect();
        Ok(self.queries()?.by_ids(viewer, ids, hidden).await)
    }

    pub async fn totals(&self, author: UserId) -> Result<Totals, PostError> {
        let (posts, likes) = self.queries()?.author_totals(author).await;
        Ok(Totals { posts, likes })
    }

    pub async fn daily(&self, role: Role, days: u32) -> Result<Vec<Day>, PostError> {
        if !role.allows(Permission::ReadStats) {
            return Err(PostError::NotAdmin);
        }
        if !(1..=MAX_DAYS).contains(&days) {
            return Err(PostError::BadDays(days));
        }
        let rows = self.queries()?.daily(days, now_micros()).await;
        Ok(rows
            .into_iter()
            .map(|(day, posts, likes)| Day { day, posts, likes })
            .collect())
    }

    pub async fn liked_by(&self, viewer: Option<UserId>, posts: &[Post]) -> HashSet<PostId> {
        let (Some(viewer), Some(queries)) = (viewer, self.queries.as_deref()) else {
            return HashSet::new();
        };
        if posts.is_empty() {
            return HashSet::new();
        }
        let ids = posts.iter().map(|post| post.id).collect();
        queries.liked(viewer, ids).await.into_iter().collect()
    }
}
