mod directory;
mod error;
mod watch;

use std::cmp::Reverse;
use std::collections::HashSet;
use std::sync::Arc;

use contracts::{CommentId, PostId, UserId};
use tokio::sync::broadcast;

use crate::common::{CommentText, Draft, Graph, Liked, Post, TRENDS_WINDOW, Trend, now_micros};
use crate::repository::PostRepository;

pub use directory::{DirectoryError, UserDirectory};
pub use error::PostError;
pub use watch::FeedWatch;

pub const MAX_TRENDS: usize = 100;

const FRESH: usize = 256;

pub struct PostService {
    posts: Arc<dyn PostRepository>,
    fresh: broadcast::Sender<Post>,
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
        PostService { posts, fresh }
    }

    pub async fn create(
        &self,
        users: &dyn UserDirectory,
        author: UserId,
        draft: Draft,
    ) -> Result<Post, PostError> {
        let Some(found) = users.author(author).await? else {
            return Err(PostError::UnknownAuthor(author));
        };
        let post = self
            .posts
            .insert(author, found.name, draft, now_micros())
            .await?;
        let _ = self.fresh.send(post.clone());
        Ok(post)
    }

    pub async fn public(&self, id: PostId) -> Result<Post, PostError> {
        self.posts.get(id).await.ok_or(PostError::UnknownPost(id))
    }

    pub async fn like(&self, user: UserId, post: PostId) -> Result<Liked, PostError> {
        let liked = self.posts.like_if_absent(user, post, now_micros()).await?;
        liked.ok_or(PostError::UnknownPost(post))
    }

    pub async fn delete(&self, user: UserId, post: PostId) -> Result<bool, PostError> {
        let found = self.public(post).await?;
        if found.author != user {
            return Err(PostError::NotAuthor(post));
        }
        Ok(self.posts.delete(post).await?)
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
    ) -> Result<CommentId, PostError> {
        let comment = self.posts.comment(author, post, text).await?;
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
