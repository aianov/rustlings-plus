use std::sync::Arc;

use social_core::{
    AuthorStats, BoxFuture, Comment, CommentId, Cursor, DomainError, Liked, PostId, PostItem,
    PostPage, PostRepo, Published, Timestamp, Trends, UserId, Visibility,
};

use super::Pace;

pub struct SlowPosts {
    inner: Arc<dyn PostRepo>,
    pace: Arc<Pace>,
}

impl SlowPosts {
    pub fn new(inner: Arc<dyn PostRepo>, pace: Arc<Pace>) -> SlowPosts {
        SlowPosts { inner, pace }
    }
}

impl PostRepo for SlowPosts {
    fn publish(
        &self,
        author: UserId,
        text: String,
        visibility: Visibility,
    ) -> BoxFuture<'_, Result<Published, DomainError>> {
        Box::pin(async move {
            self.pace.wait().await;
            self.inner.publish(author, text, visibility).await
        })
    }

    fn like(&self, user: UserId, post: PostId) -> BoxFuture<'_, Result<Liked, DomainError>> {
        Box::pin(async move {
            self.pace.wait().await;
            self.inner.like(user, post).await
        })
    }

    fn delete(&self, who: UserId, post: PostId) -> BoxFuture<'_, Result<bool, DomainError>> {
        Box::pin(async move {
            self.pace.wait().await;
            self.inner.delete(who, post).await
        })
    }

    fn post(&self, id: PostId) -> BoxFuture<'_, Option<PostItem>> {
        Box::pin(async move {
            self.pace.wait().await;
            self.inner.post(id).await
        })
    }

    fn feed(&self, viewer: UserId, skip: usize, take: usize) -> BoxFuture<'_, Vec<PostItem>> {
        Box::pin(async move {
            self.pace.wait().await;
            self.inner.feed(viewer, skip, take).await
        })
    }

    fn by_author(
        &self,
        viewer: UserId,
        author: UserId,
        after: Option<Cursor>,
        size: usize,
    ) -> BoxFuture<'_, Result<PostPage, DomainError>> {
        Box::pin(async move {
            self.pace.wait().await;
            self.inner.by_author(viewer, author, after, size).await
        })
    }

    fn author_stats(&self, author: UserId) -> BoxFuture<'_, AuthorStats> {
        Box::pin(async move {
            self.pace.wait().await;
            self.inner.author_stats(author).await
        })
    }

    fn comment(
        &self,
        author: UserId,
        post: PostId,
        text: String,
    ) -> BoxFuture<'_, Result<CommentId, DomainError>> {
        Box::pin(async move {
            self.pace.wait().await;
            self.inner.comment(author, post, text).await
        })
    }

    fn reply(
        &self,
        author: UserId,
        parent: CommentId,
        text: String,
    ) -> BoxFuture<'_, Result<CommentId, DomainError>> {
        Box::pin(async move {
            self.pace.wait().await;
            self.inner.reply(author, parent, text).await
        })
    }

    fn replies(
        &self,
        viewer: UserId,
        post: PostId,
        parent: Option<CommentId>,
    ) -> BoxFuture<'_, Result<Vec<Comment>, DomainError>> {
        Box::pin(async move {
            self.pace.wait().await;
            self.inner.replies(viewer, post, parent).await
        })
    }

    fn trend_snapshot(&self) -> BoxFuture<'_, (Trends, Timestamp)> {
        Box::pin(async move {
            self.pace.wait().await;
            self.inner.trend_snapshot().await
        })
    }

    fn count(&self) -> BoxFuture<'_, usize> {
        Box::pin(async move {
            self.pace.wait().await;
            self.inner.count().await
        })
    }
}
