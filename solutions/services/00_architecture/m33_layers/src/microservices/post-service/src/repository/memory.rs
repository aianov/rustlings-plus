use std::collections::{HashMap, HashSet};

use contracts::{CommentId, PostId, UserId};
use journal::{Journal, JournalError};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use super::{BoxFuture, PostRepository};
use crate::common::{Comment, Liked, Post};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum PostEntry {
    Posted {
        id: u64,
        author: u64,
        author_name: String,
        text: String,
        at: u64,
    },
    Liked {
        post: u64,
        by: u64,
        at: u64,
    },
    Deleted {
        post: u64,
    },
    Commented {
        id: u64,
        post: u64,
        author: u64,
        text: String,
    },
}

#[derive(Default)]
struct State {
    posts: Vec<Post>,
    likers: HashMap<PostId, HashSet<UserId>>,
    likes: Vec<(PostId, u64)>,
    comments: Vec<Comment>,
}

impl State {
    fn index(id: PostId) -> Option<usize> {
        usize::try_from(id.0).ok()?.checked_sub(1)
    }

    fn post_mut(&mut self, id: PostId) -> Option<&mut Post> {
        let index = State::index(id)?;
        self.posts.get_mut(index).filter(|post| !post.deleted)
    }

    fn post(&self, id: PostId) -> Option<&Post> {
        let index = State::index(id)?;
        self.posts.get(index).filter(|post| !post.deleted)
    }

    fn apply(&mut self, entry: PostEntry) {
        match entry {
            PostEntry::Posted {
                id,
                author,
                author_name,
                text,
                at,
            } => self.posts.push(Post {
                id: PostId(id),
                author: UserId(author),
                author_name,
                text,
                likes: 0,
                at,
                deleted: false,
            }),
            PostEntry::Liked { post, by, at } => {
                self.like(UserId(by), PostId(post), at);
            }
            PostEntry::Deleted { post } => {
                if let Some(post) = self.post_mut(PostId(post)) {
                    post.deleted = true;
                }
            }
            PostEntry::Commented {
                id,
                post,
                author,
                text,
            } => self.comments.push(Comment {
                id: CommentId(id),
                post: PostId(post),
                author: UserId(author),
                text,
            }),
        }
    }

    fn like(&mut self, user: UserId, id: PostId, at: u64) -> bool {
        if !self.likers.entry(id).or_default().insert(user) {
            return false;
        }
        if let Some(post) = self.post_mut(id) {
            post.likes += 1;
        }
        self.likes.push((id, at));
        true
    }
}

pub struct MemoryPosts {
    state: Mutex<State>,
    journal: Journal<PostEntry>,
}

impl MemoryPosts {
    pub fn new() -> MemoryPosts {
        MemoryPosts::restore(Journal::memory(), Vec::new())
    }

    pub fn restore(journal: Journal<PostEntry>, entries: Vec<PostEntry>) -> MemoryPosts {
        let mut state = State::default();
        for entry in entries {
            state.apply(entry);
        }
        MemoryPosts {
            state: Mutex::new(state),
            journal,
        }
    }
}

impl Default for MemoryPosts {
    fn default() -> MemoryPosts {
        MemoryPosts::new()
    }
}

impl PostRepository for MemoryPosts {
    fn insert(
        &self,
        author: UserId,
        author_name: String,
        text: String,
        at: u64,
    ) -> BoxFuture<'_, Result<Post, JournalError>> {
        Box::pin(async move {
            let mut state = self.state.lock().await;
            let id = state.posts.len() as u64 + 1;
            let entry = PostEntry::Posted {
                id,
                author: author.0,
                author_name,
                text,
                at,
            };
            self.journal.append(&entry).await?;
            state.apply(entry);
            let posted = state.posts.last().cloned();
            posted.ok_or_else(|| JournalError::Broken {
                line: 0,
                reason: String::from("пост пропал сразу после записи"),
            })
        })
    }

    fn get(&self, id: PostId) -> BoxFuture<'_, Option<Post>> {
        Box::pin(async move { self.state.lock().await.post(id).cloned() })
    }

    fn like_if_absent(
        &self,
        user: UserId,
        post: PostId,
        at: u64,
    ) -> BoxFuture<'_, Result<Option<Liked>, JournalError>> {
        Box::pin(async move {
            let mut state = self.state.lock().await;
            let Some(author) = state.post(post).map(|found| found.author) else {
                return Ok(None);
            };
            if state
                .likers
                .get(&post)
                .is_some_and(|likers| likers.contains(&user))
            {
                return Ok(Some(Liked {
                    changed: false,
                    author,
                }));
            }
            let entry = PostEntry::Liked {
                post: post.0,
                by: user.0,
                at,
            };
            self.journal.append(&entry).await?;
            let changed = state.like(user, post, at);
            Ok(Some(Liked { changed, author }))
        })
    }

    fn delete(&self, post: PostId) -> BoxFuture<'_, Result<bool, JournalError>> {
        Box::pin(async move {
            let mut state = self.state.lock().await;
            if state.post(post).is_none() {
                return Ok(false);
            }
            let entry = PostEntry::Deleted { post: post.0 };
            self.journal.append(&entry).await?;
            state.apply(entry);
            Ok(true)
        })
    }

    fn by_authors(
        &self,
        authors: Vec<UserId>,
        skip: usize,
        take: usize,
    ) -> BoxFuture<'_, Vec<Post>> {
        Box::pin(async move {
            let wanted: HashSet<UserId> = authors.into_iter().collect();
            let state = self.state.lock().await;
            state
                .posts
                .iter()
                .rev()
                .filter(|post| !post.deleted && wanted.contains(&post.author))
                .skip(skip)
                .take(take)
                .cloned()
                .collect()
        })
    }

    fn comment(
        &self,
        author: UserId,
        post: PostId,
        text: String,
    ) -> BoxFuture<'_, Result<Option<CommentId>, JournalError>> {
        Box::pin(async move {
            let mut state = self.state.lock().await;
            if state.post(post).is_none() {
                return Ok(None);
            }
            let id = state.comments.len() as u64 + 1;
            let entry = PostEntry::Commented {
                id,
                post: post.0,
                author: author.0,
                text,
            };
            self.journal.append(&entry).await?;
            state.apply(entry);
            Ok(Some(CommentId(id)))
        })
    }

    fn likes_since(&self, since: u64) -> BoxFuture<'_, Vec<(PostId, usize)>> {
        Box::pin(async move {
            let state = self.state.lock().await;
            let mut counts: HashMap<PostId, usize> = HashMap::new();
            for (post, at) in &state.likes {
                if *at >= since && state.post(*post).is_some() {
                    *counts.entry(*post).or_default() += 1;
                }
            }
            counts.into_iter().collect()
        })
    }
}
