use std::sync::Arc;

use contracts::{CommentId, PostId, UserId};

use super::{PostError, PostService};
use crate::common::{Post, now_micros};
use crate::repository::{PinQueries, Pinned};

impl PostService {
    pub fn with_pins(self, pins: Arc<dyn PinQueries>) -> PostService {
        PostService {
            pins: Some(pins),
            ..self
        }
    }

    fn pins(&self) -> Result<&dyn PinQueries, PostError> {
        self.pins.as_deref().ok_or(PostError::NoThreads)
    }

    pub async fn pin(&self, author: UserId, post: PostId, on: bool) -> Result<bool, PostError> {
        match self.pins()?.pin(author, post, on).await? {
            Pinned::Changed(changed) => Ok(changed),
            Pinned::Missing => Err(PostError::UnknownPost(post)),
            Pinned::NotAuthor => Err(PostError::NotAuthor(post)),
            Pinned::Full => Err(PostError::TooManyPins),
        }
    }

    pub async fn pinned(&self, author: UserId) -> Result<Vec<Post>, PostError> {
        let ids = self.pins()?.pinned(author).await;
        let mut posts = Vec::with_capacity(ids.len());
        for id in ids {
            if let Ok(post) = self.public(id).await {
                posts.push(post);
            }
        }
        Ok(posts)
    }

    pub async fn like_comment(
        &self,
        user: UserId,
        comment: CommentId,
    ) -> Result<(bool, u64), PostError> {
        let liked = self
            .pins()?
            .like_comment(user, comment, now_micros())
            .await?;
        liked.ok_or(PostError::UnknownComment(comment))
    }
}
