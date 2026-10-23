use std::collections::HashMap;
use std::sync::Arc;

use contracts::{Cause, CommentId, Origin, PostId, UserId};

use super::{PostError, PostService};
use crate::common::{CommentText, now_micros};
use crate::repository::{Line, Node, Rank, ThreadQueries};

pub const MAX_DEPTH: u32 = 32;

pub const MAX_TOP: u32 = 50;

impl PostService {
    pub fn with_threads(self, threads: Arc<dyn ThreadQueries>) -> PostService {
        PostService {
            threads: Some(threads),
            ..self
        }
    }

    fn threads(&self) -> Result<&dyn ThreadQueries, PostError> {
        self.threads.as_deref().ok_or(PostError::NoThreads)
    }

    pub async fn reply_to(
        &self,
        author: UserId,
        post: PostId,
        parent: CommentId,
        text: CommentText,
        origin: &Origin,
    ) -> Result<CommentId, PostError> {
        let threads = self.threads()?;
        let cause = Cause::new(origin, now_micros());
        match threads.reply(author, post, parent, text, &cause).await? {
            Some(comment) => Ok(comment),
            None => match self.public(post).await {
                Ok(..) => Err(PostError::UnknownParent(parent)),
                Err(error) => Err(error),
            },
        }
    }

    pub async fn thread(&self, post: PostId, depth: u32) -> Result<Vec<Node>, PostError> {
        let depth = match depth {
            0 => MAX_DEPTH,
            depth => depth.min(MAX_DEPTH),
        };
        self.public(post).await?;
        Ok(self.threads()?.thread(post, depth).await)
    }

    pub async fn latest(&self, posts: &[PostId]) -> HashMap<PostId, Vec<Line>> {
        let Some(threads) = self.threads.as_deref() else {
            return HashMap::new();
        };
        threads.latest(posts.to_vec()).await.into_iter().collect()
    }

    pub async fn top(&self, limit: u32) -> Result<Vec<Rank>, PostError> {
        let limit = match limit {
            0 => 10,
            limit => limit.min(MAX_TOP),
        };
        Ok(self.threads()?.top(now_micros(), limit).await)
    }
}
