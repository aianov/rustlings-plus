mod directory;
mod error;
mod idempotency;
mod queries;
mod watch;

use std::cmp::Reverse;
use std::collections::HashSet;
use std::sync::Arc;

use contracts::{Cause, CommentId, Origin, Permission, PostId, UserId};
use tokio::sync::broadcast;

use crate::common::{
    Actor, CommentText, Draft, Graph, Liked, Post, PostText, TRENDS_WINDOW, Trend, now_micros,
};
use crate::repository::{DraftRepository, PostQueries, PostRepository};

pub use directory::{DirectoryError, UserDirectory};
pub use error::PostError;
pub use idempotency::{Idempotency, KEPT_KEYS, MAX_KEY, good_key};
pub use queries::{MAX_DAYS, MAX_IDS};
pub use watch::FeedWatch;

pub const MAX_TRENDS: usize = 100;

const FRESH: usize = 256;

pub struct PostService {
    posts: Arc<dyn PostRepository>,
    fresh: broadcast::Sender<Post>,
    once: Idempotency,
    drafts: Option<Arc<dyn DraftRepository>>,
    queries: Option<Arc<dyn PostQueries>>,
}

fn readers(viewer: UserId, graph: &Graph) -> Vec<UserId> {
    let muted: HashSet<UserId> = graph.muted.iter().copied().collect();
    let mut authors: Vec<UserId> = graph
        .following
        .iter()
        .copied()
        .filter(|author| !muted.contains(author))
        .collect();
    authors.push(viewer);
    authors
}

impl PostService {
    pub fn new(posts: Arc<dyn PostRepository>) -> PostService {
        let (fresh, _) = broadcast::channel(FRESH);
        PostService {
            posts,
            fresh,
            once: Idempotency::default(),
            drafts: None,
            queries: None,
        }
    }

    pub fn with_drafts(self, drafts: Arc<dyn DraftRepository>) -> PostService {
        PostService {
            drafts: Some(drafts),
            ..self
        }
    }

    fn drafts(&self) -> Result<&dyn DraftRepository, PostError> {
        self.drafts.as_deref().ok_or(PostError::NoDrafts)
    }

    pub async fn delete_comment(
        &self,
        actor: impl Into<Actor>,
        comment: CommentId,
        origin: &Origin,
    ) -> Result<bool, PostError> {
        let drafts = self.drafts()?;
        let actor = actor.into();
        let Some((writer, owner)) = drafts.comment_owners(comment).await else {
            return Err(PostError::UnknownComment(comment));
        };
        let allowed =
            actor.id == writer || actor.id == owner || actor.role.allows(Permission::DeleteAnyPost);
        if !allowed {
            return Err(PostError::NotCommentOwner(comment));
        }
        let cause = Cause::new(origin, now_micros());
        Ok(drafts.remove_comment(comment, &cause).await?)
    }

    pub async fn save_draft(
        &self,
        owner: UserId,
        text: PostText,
        origin: &Origin,
    ) -> Result<u64, PostError> {
        let cause = Cause::new(origin, now_micros());
        let text = text.as_ref().to_string();
        Ok(self.drafts()?.save_draft(owner, text, &cause).await?)
    }

    pub async fn draft(&self, owner: UserId, id: u64) -> Result<String, PostError> {
        match self.drafts()?.draft(id).await {
            Some((own, text)) if own == owner => Ok(text),
            Some(..) | None => Err(PostError::UnknownDraft(id)),
        }
    }

    pub async fn delete_draft(
        &self,
        owner: UserId,
        id: u64,
        origin: &Origin,
    ) -> Result<bool, PostError> {
        self.draft(owner, id).await?;
        let cause = Cause::new(origin, now_micros());
        Ok(self.drafts()?.delete_draft(id, &cause).await?)
    }

    pub async fn create_once(
        &self,
        users: &dyn UserDirectory,
        author: UserId,
        key: Option<&str>,
        draft: Draft,
        origin: &Origin,
    ) -> Result<Post, PostError> {
        let Some(key) = key.filter(|key| !key.is_empty()) else {
            return self.create(users, author, draft, origin).await;
        };
        let cell = self.once.cell(author, key);
        cell.get_or_try_init(|| self.create(users, author, draft, origin))
            .await
            .cloned()
    }

    pub fn remembered_keys(&self) -> usize {
        self.once.len()
    }

    pub async fn create(
        &self,
        users: &dyn UserDirectory,
        author: UserId,
        draft: Draft,
        origin: &Origin,
    ) -> Result<Post, PostError> {
        let Some(found) = users.author(author).await? else {
            return Err(PostError::UnknownAuthor(author));
        };
        let cause = Cause::new(origin, now_micros());
        let post = self.posts.insert(author, found.name, draft, &cause).await?;
        let _ = self.fresh.send(post.clone());
        Ok(post)
    }

    pub async fn public(&self, id: PostId) -> Result<Post, PostError> {
        self.posts.get(id).await.ok_or(PostError::UnknownPost(id))
    }

    pub async fn like(
        &self,
        user: UserId,
        post: PostId,
        origin: &Origin,
    ) -> Result<Liked, PostError> {
        let cause = Cause::new(origin, now_micros());
        let liked = self.posts.like_if_absent(user, post, &cause).await?;
        liked.ok_or(PostError::UnknownPost(post))
    }

    pub async fn edit(
        &self,
        actor: impl Into<Actor>,
        post: PostId,
        text: PostText,
        origin: &Origin,
    ) -> Result<Post, PostError> {
        let actor = actor.into();
        let found = self.public(post).await?;
        let own = found.author == actor.id && actor.role.allows(Permission::EditOwnPost);
        if !own {
            return Err(PostError::NotAuthor(post));
        }
        let cause = Cause::new(origin, now_micros());
        let edited = self.posts.edit(post, text, &cause).await?;
        edited.ok_or(PostError::UnknownPost(post))
    }

    pub async fn delete(
        &self,
        actor: impl Into<Actor>,
        post: PostId,
        origin: &Origin,
    ) -> Result<bool, PostError> {
        let actor = actor.into();
        let found = self.public(post).await?;
        let own = found.author == actor.id && actor.role.allows(Permission::DeleteOwnPost);
        if !own && !actor.role.allows(Permission::DeleteAnyPost) {
            return Err(PostError::NotAuthor(post));
        }
        let cause = Cause::new(origin, now_micros());
        Ok(self.posts.delete(post, &cause).await?)
    }

    pub async fn by_author(&self, author: UserId, limit: usize) -> Vec<Post> {
        self.posts.by_authors(vec![author], 0, limit).await
    }

    async fn graph(&self, users: &dyn UserDirectory, viewer: UserId) -> Result<Graph, PostError> {
        users
            .graph(viewer)
            .await?
            .ok_or(PostError::UnknownUser(viewer))
    }

    pub async fn feed(
        &self,
        users: &dyn UserDirectory,
        viewer: UserId,
        page: usize,
        size: usize,
    ) -> Result<Vec<Post>, PostError> {
        let graph = self.graph(users, viewer).await?;
        let skip = page
            .checked_sub(1)
            .and_then(|before| before.checked_mul(size));
        match skip {
            Some(skip) if size > 0 => Ok(self
                .posts
                .by_authors(readers(viewer, &graph), skip, size)
                .await),
            _ => Ok(Vec::new()),
        }
    }

    pub async fn comment(
        &self,
        author: UserId,
        post: PostId,
        text: CommentText,
        origin: &Origin,
    ) -> Result<CommentId, PostError> {
        let cause = Cause::new(origin, now_micros());
        let comment = self.posts.comment(author, post, text, &cause).await?;
        comment.ok_or(PostError::UnknownPost(post))
    }

    pub async fn trends(&self, limit: usize) -> Vec<Trend> {
        let since = now_micros().saturating_sub(TRENDS_WINDOW);
        let mut counts = self.posts.likes_since(since).await;
        counts.sort_by_key(|&(post, likes)| (Reverse(likes), Reverse(post)));
        let limit = if limit == 0 {
            MAX_TRENDS
        } else {
            limit.min(MAX_TRENDS)
        };
        counts
            .into_iter()
            .take(limit)
            .map(|(post, likes)| Trend { post, likes })
            .collect()
    }

    pub async fn watch(
        &self,
        users: &dyn UserDirectory,
        viewer: UserId,
        recent: usize,
    ) -> Result<FeedWatch, PostError> {
        let live = self.fresh.subscribe();
        let graph = self.graph(users, viewer).await?;
        let authors = readers(viewer, &graph);
        let history = self.posts.by_authors(authors.clone(), 0, recent).await;
        Ok(FeedWatch::new(history, authors, live))
    }
}
