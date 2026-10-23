use std::collections::{BTreeMap, BTreeSet};

use contracts::{PostId, UserId};
use tokio::sync::Mutex;

use super::state::State;
use super::{BoxFuture, FeedRepository};
use crate::common::{Change, Consumed, Cursor, FeedItem, FeedLimits};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub names: BTreeMap<UserId, String>,
    pub follows: BTreeSet<(UserId, UserId)>,
    pub mutes: BTreeSet<(UserId, UserId)>,
    pub posts: BTreeMap<PostId, (UserId, String, usize, bool)>,
    pub waiting: usize,
}

#[derive(Default)]
pub struct MemoryFeed {
    state: Mutex<State>,
}

impl MemoryFeed {
    pub fn new() -> MemoryFeed {
        MemoryFeed::default()
    }

    pub fn with_limits(limits: FeedLimits) -> MemoryFeed {
        let state = State {
            limits,
            ..State::default()
        };
        MemoryFeed {
            state: Mutex::new(state),
        }
    }
}

fn cursor_of(item: &FeedItem) -> Cursor {
    Cursor {
        at: item.at,
        post: item.post,
    }
}

impl FeedRepository for MemoryFeed {
    fn apply(&self, aggregate: String, version: u64, change: Change) -> BoxFuture<'_, Consumed> {
        Box::pin(async move {
            let mut state = self.state.lock().await;
            let expected = state.next.get(&aggregate).copied().unwrap_or(1);
            if version < expected {
                return Consumed::Duplicate;
            }
            if version > expected {
                let held = state.held.entry(aggregate).or_default();
                if held.contains_key(&version) {
                    return Consumed::Duplicate;
                }
                held.insert(version, change);
                return Consumed::Buffered;
            }
            state.effect(change);
            let mut next = expected + 1;
            loop {
                let waiting = state
                    .held
                    .get_mut(&aggregate)
                    .and_then(|held| held.remove(&next));
                let Some(change) = waiting else { break };
                state.effect(change);
                next += 1;
            }
            if state.held.get(&aggregate).is_some_and(BTreeMap::is_empty) {
                state.held.remove(&aggregate);
            }
            state.next.insert(aggregate, next);
            Consumed::Applied
        })
    }

    fn page(&self, viewer: UserId, skip: usize, take: usize) -> BoxFuture<'_, Vec<FeedItem>> {
        Box::pin(async move {
            let state = self.state.lock().await;
            let wanted = skip.saturating_add(take);
            state
                .after(viewer, None, wanted)
                .into_iter()
                .skip(skip)
                .take(take)
                .collect()
        })
    }

    fn after(
        &self,
        viewer: UserId,
        before: Option<Cursor>,
        take: usize,
    ) -> BoxFuture<'_, (Vec<FeedItem>, Option<Cursor>)> {
        Box::pin(async move {
            let state = self.state.lock().await;
            let items = state.after(viewer, before, take.saturating_add(1));
            let more = items.len() > take;
            let items: Vec<FeedItem> = items.into_iter().take(take).collect();
            let next = if more {
                items.last().map(cursor_of)
            } else {
                None
            };
            (items, next)
        })
    }

    fn drain(&self, budget: usize) -> BoxFuture<'_, usize> {
        Box::pin(async move { self.state.lock().await.drain(budget) })
    }

    fn pending(&self) -> BoxFuture<'_, usize> {
        Box::pin(async move {
            let state = self.state.lock().await;
            state
                .pending
                .iter()
                .map(|delivery| delivery.readers.len())
                .sum()
        })
    }

    fn snapshot(&self) -> BoxFuture<'_, Snapshot> {
        Box::pin(async move {
            let state = self.state.lock().await;
            Snapshot {
                names: state
                    .authors
                    .iter()
                    .map(|(user, author)| {
                        let name = author
                            .display_name
                            .clone()
                            .unwrap_or_else(|| author.username.clone());
                        (*user, name)
                    })
                    .collect(),
                follows: state.follows.clone(),
                mutes: state.mutes.clone(),
                posts: state
                    .posts
                    .iter()
                    .map(|(post, item)| {
                        let shown = (item.author, item.text.clone(), item.likes, item.deleted);
                        (*post, shown)
                    })
                    .collect(),
                waiting: state.held.values().map(BTreeMap::len).sum(),
            }
        })
    }
}
