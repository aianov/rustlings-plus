use crate::chat::Message;
use crate::comments::Comment;
use crate::domain::{CommentId, Cursor, PostId, User, UserId};
use crate::ports::{AuthorStats, BoxFuture, FollowCounts, PostItem, PostPage};

use super::{App, AppError};

impl App {
    pub async fn user(&self, id: UserId) -> Option<User> {
        self.inner.repos.users.user(id).await
    }

    pub async fn find_user(&self, name: &str) -> Option<User> {
        self.inner.repos.users.find(name.to_string()).await
    }

    pub async fn user_count(&self) -> usize {
        self.inner.repos.users.count().await
    }

    pub async fn is_following(&self, who: UserId, whom: UserId) -> bool {
        self.inner.repos.users.is_following(who, whom).await
    }

    pub async fn follow_counts(&self, id: UserId) -> FollowCounts {
        self.inner.repos.users.follow_counts(id).await
    }

    pub async fn post(&self, id: PostId) -> Option<PostItem> {
        self.inner.repos.posts.post(id).await
    }

    pub async fn post_count(&self) -> usize {
        self.inner.repos.posts.count().await
    }

    pub async fn author_stats(&self, author: UserId) -> AuthorStats {
        self.inner.repos.posts.author_stats(author).await
    }

    pub async fn feed(&self, viewer: UserId, skip: usize, take: usize) -> Vec<PostItem> {
        self.inner.repos.posts.feed(viewer, skip, take).await
    }

    pub async fn author_page(
        &self,
        viewer: UserId,
        author: UserId,
        after: Option<Cursor>,
        size: usize,
    ) -> Result<PostPage, AppError> {
        let posts = &self.inner.repos.posts;
        Ok(posts.by_author(viewer, author, after, size).await?)
    }

    pub async fn history(
        &self,
        user: UserId,
        peer: UserId,
        limit: usize,
    ) -> Result<Vec<Message>, AppError> {
        Ok(self.inner.repos.chats.history(user, peer, limit).await?)
    }

    pub async fn thread(
        &self,
        viewer: UserId,
        post: PostId,
    ) -> Result<Vec<(usize, Comment)>, AppError> {
        let mut thread = Vec::new();
        self.branch(viewer, post, None, 0, &mut thread).await?;
        Ok(thread)
    }

    fn branch<'a>(
        &'a self,
        viewer: UserId,
        post: PostId,
        parent: Option<CommentId>,
        depth: usize,
        thread: &'a mut Vec<(usize, Comment)>,
    ) -> BoxFuture<'a, Result<(), AppError>> {
        Box::pin(async move {
            let replies = self.inner.repos.posts.replies(viewer, post, parent).await?;
            for comment in replies {
                let id = comment.id();
                thread.push((depth, comment));
                self.branch(viewer, post, Some(id), depth + 1, thread)
                    .await?;
            }
            Ok(())
        })
    }
}
