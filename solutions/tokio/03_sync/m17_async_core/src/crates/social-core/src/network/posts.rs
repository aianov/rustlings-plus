use crate::domain::{
    Cursor, DomainError, Post, PostId, Timestamp, UserId, Visibility, validate_text,
};
use crate::events::Event;
use crate::ports::{PostStore, UserStore};
use crate::trends::{Trend, Trends};

use super::{AuthorPage, Network};

impl<U: UserStore, P: PostStore> Network<U, P> {
    pub fn publish(
        &mut self,
        author: UserId,
        text: &str,
        visibility: Visibility,
    ) -> Result<PostId, DomainError> {
        self.publish_at(author, text, visibility, self.now())
    }

    pub fn publish_at(
        &mut self,
        author: UserId,
        text: &str,
        visibility: Visibility,
        at: Timestamp,
    ) -> Result<PostId, DomainError> {
        self.ensure_user(author)?;
        let text = validate_text(text)?;
        let id = self.posts.next_id();
        let post = Post::new(id, author, text, visibility).at(at);
        self.authors.add(author, post.cursor());
        self.tags.add(id, post.text());
        self.posts.insert(post);
        self.events.publish(&Event::NewPost { author, post: id });
        Ok(id)
    }

    pub fn audience(&self, post: PostId) -> Vec<UserId> {
        let Some(post) = self.posts.get(post).filter(|post| !post.is_deleted()) else {
            return Vec::new();
        };
        let author = post.author();
        self.follows
            .followers(author)
            .iter()
            .copied()
            .filter(|&follower| post.can_view(follower, true))
            .filter(|&follower| !self.mutes.contains(follower, author))
            .collect()
    }

    pub fn post_count(&self) -> usize {
        self.posts.count()
    }

    pub fn post(&self, id: PostId) -> Option<&Post> {
        self.posts.get(id)
    }

    pub fn last_post(&self, author: UserId) -> Option<&Post> {
        self.posts.last_by(author)
    }

    pub fn like(&mut self, user: UserId, post: PostId) -> Result<bool, DomainError> {
        self.like_at(user, post, self.now())
    }

    pub fn like_at(
        &mut self,
        user: UserId,
        post: PostId,
        at: Timestamp,
    ) -> Result<bool, DomainError> {
        self.ensure_user(user)?;
        let target = self
            .posts
            .get_mut(post)
            .filter(|target| !target.is_deleted())
            .ok_or(DomainError::UnknownPost(post))?;
        let author = target.author();
        if !target.like(user) {
            return Ok(false);
        }
        if target.visibility() == Visibility::Public {
            self.trends.record(post, at);
        }
        self.events.publish(&Event::Liked {
            by: user,
            post,
            author,
        });
        Ok(true)
    }

    pub fn likes(&self, post: PostId) -> usize {
        match self.posts.get(post) {
            Some(post) => post.likes(),
            None => 0,
        }
    }

    pub fn delete_post(&mut self, who: UserId, post: PostId) -> Result<bool, DomainError> {
        self.ensure_user(who)?;
        let target = self
            .posts
            .get_mut(post)
            .ok_or(DomainError::UnknownPost(post))?;
        if target.author() != who {
            return Err(DomainError::NotAuthor(post));
        }
        if !target.delete() {
            return Ok(false);
        }
        self.authors.remove(who, target.cursor());
        self.tags.remove(post, target.text());
        self.trends.forget(post);
        self.comments.forget_post(post);
        Ok(true)
    }

    pub fn posts_with_tag(&self, tag: &str) -> Vec<PostId> {
        self.tags.posts(tag).collect()
    }

    pub fn author_page(
        &self,
        viewer: UserId,
        author: UserId,
        after: Option<Cursor>,
        size: usize,
    ) -> Result<AuthorPage<'_>, DomainError> {
        self.ensure_user(viewer)?;
        self.ensure_user(author)?;
        let follows = self.follows.contains(viewer, author);
        let mut visible = self
            .authors
            .newest_first(author, after)
            .filter_map(|key| self.posts.get(key.post))
            .filter(|post| post.can_view(viewer, follows));
        let posts: Vec<&Post> = visible.by_ref().take(size).collect();
        let next = match (posts.last(), visible.next()) {
            (Some(last), Some(_)) => Some(last.cursor()),
            _ => None,
        };
        Ok(AuthorPage { posts, next })
    }

    pub fn trending(&self, now: Timestamp, window: u64, n: usize) -> Vec<Trend> {
        self.trends.top(now, window, n)
    }

    pub(crate) fn trend_likes(&self) -> Trends {
        self.trends.clone()
    }
}
