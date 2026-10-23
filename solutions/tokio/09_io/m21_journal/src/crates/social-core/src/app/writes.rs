use contracts::Entry;

use super::{App, AppError, Op};
use crate::chat::Message;
use crate::domain::{CommentId, DomainError, MAX_POST_LEN, PostId, UserId, Visibility};
use crate::events::Event;

impl App {
    pub async fn register(&self, name: &str) -> Result<UserId, AppError> {
        let _order = self.inner.journal.order().await;
        let user = self.inner.repos.users.register(name.to_string()).await?;
        let entry = || Entry::Registered {
            user,
            name: name.to_string(),
        };
        self.inner.journal.record(entry).await?;
        Ok(user)
    }

    pub async fn publish(
        &self,
        author: UserId,
        text: &str,
        visibility: Visibility,
    ) -> Result<PostId, AppError> {
        if self.is_closed() {
            return Err(AppError::Stopped(Op::Flush));
        }
        let limit = self.settings().post_limit;
        if limit < MAX_POST_LEN && text.trim().chars().count() > limit {
            return Err(DomainError::PostOverLimit(limit).into());
        }
        let order = self.inner.journal.order().await;
        let posts = &self.inner.repos.posts;
        let published = posts.publish(author, text.to_string(), visibility).await?;
        let (post, at) = (published.post, published.at);
        let entry = || Entry::Posted {
            post,
            author,
            text: text.to_string(),
            visibility,
            at,
        };
        self.inner.journal.record(entry).await?;
        drop(order);
        self.emit(Event::NewPost { author, post });
        self.inner
            .fanout
            .published(author, post, published.audience)
            .await?;
        Ok(post)
    }

    pub async fn follow(&self, who: UserId, whom: UserId) -> Result<bool, AppError> {
        let order = self.inner.journal.order().await;
        let added = self.inner.repos.users.follow(who, whom).await?;
        if added {
            let entry = || Entry::Followed { who, whom };
            self.inner.journal.record(entry).await?;
            drop(order);
            self.emit(Event::Followed { who, whom });
        }
        Ok(added)
    }

    pub async fn like(&self, user: UserId, post: PostId) -> Result<bool, AppError> {
        let order = self.inner.journal.order().await;
        let liked = self.inner.repos.posts.like(user, post).await?;
        if liked.changed {
            let at = liked.at;
            let entry = || Entry::Liked { post, by: user, at };
            self.inner.journal.record(entry).await?;
            drop(order);
            self.emit(Event::Liked {
                by: user,
                post,
                author: liked.author,
            });
        }
        Ok(liked.changed)
    }

    pub async fn mute(&self, who: UserId, whom: UserId) -> Result<bool, AppError> {
        let _order = self.inner.journal.order().await;
        let added = self.inner.repos.users.mute(who, whom).await?;
        if added {
            let entry = || Entry::Muted { who, whom };
            self.inner.journal.record(entry).await?;
        }
        Ok(added)
    }

    pub async fn delete_post(&self, who: UserId, post: PostId) -> Result<bool, AppError> {
        let _order = self.inner.journal.order().await;
        let deleted = self.inner.repos.posts.delete(who, post).await?;
        if deleted {
            let entry = || Entry::Deleted { post, by: who };
            self.inner.journal.record(entry).await?;
        }
        Ok(deleted)
    }

    pub async fn comment(
        &self,
        author: UserId,
        post: PostId,
        text: &str,
    ) -> Result<CommentId, AppError> {
        let _order = self.inner.journal.order().await;
        let posts = &self.inner.repos.posts;
        let comment = posts.comment(author, post, text.to_string()).await?;
        let entry = || Entry::Commented {
            comment,
            post,
            author,
            text: text.to_string(),
        };
        self.inner.journal.record(entry).await?;
        Ok(comment)
    }

    pub async fn reply(
        &self,
        author: UserId,
        parent: CommentId,
        text: &str,
    ) -> Result<CommentId, AppError> {
        let _order = self.inner.journal.order().await;
        let posts = &self.inner.repos.posts;
        let comment = posts.reply(author, parent, text.to_string()).await?;
        let entry = || Entry::Replied {
            comment,
            parent,
            author,
            text: text.to_string(),
        };
        self.inner.journal.record(entry).await?;
        Ok(comment)
    }

    pub async fn send_message(
        &self,
        sender: UserId,
        recipient: UserId,
        text: &str,
    ) -> Result<Message, AppError> {
        let order = self.inner.journal.order().await;
        let chats = &self.inner.repos.chats;
        let message = chats.send(sender, recipient, text.to_string()).await?;
        let entry = || Entry::MessageSent {
            from: sender,
            to: recipient,
            text: text.to_string(),
        };
        self.inner.journal.record(entry).await?;
        drop(order);
        self.emit(Event::MessageSent {
            from: sender,
            to: recipient,
        });
        Ok(message)
    }
}
