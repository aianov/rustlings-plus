use contracts::UserId;

use super::{LIST_LIMIT, UserError, UserService};
use crate::common::User;
use crate::repository::SocialQueries;

pub enum Related {
    Followers,
    Following,
    Friends,
}

fn limited(limit: usize) -> usize {
    if limit == 0 {
        LIST_LIMIT
    } else {
        limit.min(LIST_LIMIT)
    }
}

impl UserService {
    fn queries(&self) -> Result<&dyn SocialQueries, UserError> {
        self.queries.as_deref().ok_or(UserError::NoQueries)
    }

    pub async fn related(
        &self,
        kind: Related,
        user: UserId,
        after: u64,
        limit: usize,
    ) -> Result<Vec<User>, UserError> {
        self.profile(user).await?;
        let queries = self.queries()?;
        let limit = limited(limit);
        Ok(match kind {
            Related::Followers => queries.followers(user, after, limit).await,
            Related::Following => queries.following(user, after, limit).await,
            Related::Friends => queries.friends(user, after, limit).await,
        })
    }

    pub async fn counters(&self, user: UserId) -> Result<(u64, u64), UserError> {
        self.profile(user).await?;
        Ok(self.queries()?.counters(user).await)
    }
}
