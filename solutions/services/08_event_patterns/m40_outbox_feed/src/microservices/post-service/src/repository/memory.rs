use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use contracts::{Cause, CommentId, PostId, UserId};
use journal::{Journal, JournalError};
use tokio::sync::{Mutex, Notify};

use super::entry::fact_of;
use super::state::State;
use super::{BoxFuture, LinkEntry, PostEntry, PostLine, PostRepository};
use crate::common::{CommentText, Draft, Liked, Post, PostText};

pub struct MemoryPosts {
    pub(super) state: Mutex<State>,
    pub(super) journal: Journal<PostLine>,
    pub(super) signal: Arc<Notify>,
}

impl MemoryPosts {
    pub fn new() -> MemoryPosts {
        MemoryPosts::restore(Journal::memory(), Vec::new())
    }

    pub fn restore(journal: Journal<PostLine>, lines: Vec<PostLine>) -> MemoryPosts {
        let mut state = State::default();
        for line in lines {
            state.apply(line.entry);
            if let Some(row) = line.outbox {
                state.outbox.push(row);
            }
        }
        MemoryPosts {
            state: Mutex::new(state),
            journal,
            signal: Arc::default(),
        }
    }

    async fn record(
        &self,
        state: &mut State,
        entry: &PostEntry,
        cause: &Cause,
    ) -> Result<(), JournalError> {
        let row = fact_of(state, entry).map(|fact| state.outbox.prepare(&fact, cause));
        let line = PostLine {
            entry: entry.clone(),
            outbox: row.clone(),
        };
        self.journal.append(&line).await?;
        if let Some(row) = row {
            state.outbox.push(row);
            self.signal.notify_one();
        }
        Ok(())
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
        draft: Draft,
        cause: &Cause,
    ) -> BoxFuture<'_, Result<Post, JournalError>> {
        let (cause, at) = (cause.clone(), cause.at);
        Box::pin(async move {
            let mut state = self.state.lock().await;
            let id = state.posts.len() as u64 + 1;
            let entry = PostEntry::Posted {
                id,
                author: author.0,
                author_name,
                text: draft.text.as_ref().to_string(),
                tags: draft
                    .tags
                    .iter()
                    .map(|tag| tag.as_ref().to_string())
                    .collect(),
                links: draft
                    .links
                    .into_iter()
                    .map(|link| LinkEntry {
                        url: link.url,
                        title: link.title,
                    })
                    .collect(),
                at,
            };
            self.record(&mut state, &entry, &cause).await?;
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
        cause: &Cause,
    ) -> BoxFuture<'_, Result<Option<Liked>, JournalError>> {
        let (cause, at) = (cause.clone(), cause.at);
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
            self.record(&mut state, &entry, &cause).await?;
            let changed = state.like(user, post, at);
            Ok(Some(Liked { changed, author }))
        })
    }

    fn edit(
        &self,
        post: PostId,
        text: PostText,
        cause: &Cause,
    ) -> BoxFuture<'_, Result<Option<Post>, JournalError>> {
        let cause = cause.clone();
        Box::pin(async move {
            let mut state = self.state.lock().await;
            if state.post(post).is_none() {
                return Ok(None);
            }
            let entry = PostEntry::Edited {
                post: post.0,
                text: text.as_ref().to_string(),
            };
            self.record(&mut state, &entry, &cause).await?;
            state.apply(entry);
            Ok(state.post(post).cloned())
        })
    }

    fn delete(&self, post: PostId, cause: &Cause) -> BoxFuture<'_, Result<bool, JournalError>> {
        let cause = cause.clone();
        Box::pin(async move {
            let mut state = self.state.lock().await;
            if state.post(post).is_none() {
                return Ok(false);
            }
            let entry = PostEntry::Deleted { post: post.0 };
            self.record(&mut state, &entry, &cause).await?;
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
        text: CommentText,
        cause: &Cause,
    ) -> BoxFuture<'_, Result<Option<CommentId>, JournalError>> {
        let cause = cause.clone();
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
                text: text.as_ref().to_string(),
            };
            self.record(&mut state, &entry, &cause).await?;
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
