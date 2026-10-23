use std::collections::{BTreeMap, BTreeSet, HashMap};

use contracts::{LinkView, PostId, UserId};
use tokio::sync::Mutex;

use super::{BoxFuture, FeedRepository};
use crate::common::{Change, Consumed, FeedItem};

#[derive(Debug, Clone, PartialEq, Eq)]
struct Author {
    username: String,
    display_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Item {
    author: UserId,
    author_name: String,
    text: String,
    tags: Vec<String>,
    links: Vec<LinkView>,
    likes: usize,
    at: u64,
    deleted: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub names: BTreeMap<UserId, String>,
    pub follows: BTreeSet<(UserId, UserId)>,
    pub mutes: BTreeSet<(UserId, UserId)>,
    pub posts: BTreeMap<PostId, (UserId, String, usize, bool)>,
    pub waiting: usize,
}

#[derive(Default)]
struct State {
    authors: HashMap<UserId, Author>,
    follows: BTreeSet<(UserId, UserId)>,
    mutes: BTreeSet<(UserId, UserId)>,
    posts: BTreeMap<PostId, Item>,
    next: HashMap<String, u64>,
    held: HashMap<String, BTreeMap<u64, Change>>,
}

impl State {
    fn effect(&mut self, change: Change) {
        match change {
            Change::Registered { user, username } => {
                let author = self.authors.entry(user).or_insert(Author {
                    username: username.clone(),
                    display_name: None,
                });
                author.username = username;
            }
            Change::Renamed { user, display_name } => {
                let author = self.authors.entry(user).or_insert(Author {
                    username: user.to_string(),
                    display_name: None,
                });
                author.display_name = display_name;
            }
            Change::Followed { follower, followee } => {
                self.follows.insert((follower, followee));
            }
            Change::Muted { user, muted } => {
                self.mutes.insert((user, muted));
            }
            Change::Posted {
                post,
                author,
                author_name,
                text,
                tags,
                links,
                at,
            } => {
                self.posts.insert(
                    post,
                    Item {
                        author,
                        author_name,
                        text,
                        tags,
                        links,
                        likes: 0,
                        at,
                        deleted: false,
                    },
                );
            }
            Change::Liked { post } => {
                if let Some(item) = self.posts.get_mut(&post) {
                    item.likes += 1;
                }
            }
            Change::Edited { post, text } => {
                if let Some(item) = self.posts.get_mut(&post) {
                    item.text = text;
                }
            }
            Change::Deleted { post } => {
                if let Some(item) = self.posts.get_mut(&post) {
                    item.deleted = true;
                }
            }
            Change::Gone { user } => {
                self.authors.remove(&user);
                self.follows
                    .retain(|(who, whom)| *who != user && *whom != user);
                self.mutes
                    .retain(|(who, whom)| *who != user && *whom != user);
                for item in self.posts.values_mut().filter(|item| item.author == user) {
                    item.deleted = true;
                }
            }
            Change::Nothing => {}
        }
    }

    fn name_of(&self, item: &Item) -> String {
        match self.authors.get(&item.author) {
            Some(author) => author
                .display_name
                .clone()
                .unwrap_or_else(|| author.username.clone()),
            None => item.author_name.clone(),
        }
    }

    fn visible(&self, viewer: UserId, author: UserId) -> bool {
        author == viewer
            || (self.follows.contains(&(viewer, author)) && !self.mutes.contains(&(viewer, author)))
    }
}

#[derive(Default)]
pub struct MemoryFeed {
    state: Mutex<State>,
}

impl MemoryFeed {
    pub fn new() -> MemoryFeed {
        MemoryFeed::default()
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
            let mut items: Vec<FeedItem> = state
                .posts
                .iter()
                .filter(|(_, item)| !item.deleted && state.visible(viewer, item.author))
                .map(|(post, item)| FeedItem {
                    post: *post,
                    author: item.author,
                    author_name: state.name_of(item),
                    text: item.text.clone(),
                    likes: item.likes,
                    tags: item.tags.clone(),
                    links: item.links.clone(),
                    at: item.at,
                })
                .collect();
            items.sort_by(|a, b| b.at.cmp(&a.at).then_with(|| b.post.0.cmp(&a.post.0)));
            items.into_iter().skip(skip).take(take).collect()
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
