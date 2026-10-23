use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};

use contracts::{LinkView, PostId, UserId};

use crate::common::{Change, Cursor, FeedItem, FeedLimits};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Author {
    pub(super) username: String,
    pub(super) display_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Item {
    pub(super) author: UserId,
    pub(super) author_name: String,
    pub(super) text: String,
    pub(super) tags: Vec<String>,
    pub(super) links: Vec<LinkView>,
    pub(super) likes: usize,
    pub(super) at: u64,
    pub(super) deleted: bool,
}

pub(super) struct Delivery {
    pub(super) key: Cursor,
    pub(super) readers: Vec<UserId>,
}

#[derive(Default)]
pub(super) struct State {
    pub(super) limits: FeedLimits,
    pub(super) authors: HashMap<UserId, Author>,
    pub(super) follows: BTreeSet<(UserId, UserId)>,
    pub(super) followers: HashMap<UserId, BTreeSet<UserId>>,
    pub(super) mutes: BTreeSet<(UserId, UserId)>,
    pub(super) posts: BTreeMap<PostId, Item>,
    pub(super) timelines: HashMap<UserId, BTreeSet<Cursor>>,
    pub(super) by_author: HashMap<UserId, BTreeSet<Cursor>>,
    pub(super) pending: VecDeque<Delivery>,
    pub(super) next: HashMap<String, u64>,
    pub(super) held: HashMap<String, BTreeMap<u64, Change>>,
}

fn keep(line: &mut BTreeSet<Cursor>, key: Cursor, max: usize) {
    line.insert(key);
    while line.len() > max.max(1) {
        line.pop_first();
    }
}

impl State {
    pub(super) fn celebrity(&self, author: UserId) -> bool {
        self.followers
            .get(&author)
            .is_some_and(|followers| followers.len() >= self.limits.celebrity)
    }

    fn deliver(&mut self, reader: UserId, key: Cursor) {
        let max = self.limits.max;
        keep(self.timelines.entry(reader).or_default(), key, max);
    }

    pub(super) fn drain(&mut self, mut budget: usize) -> usize {
        let mut done = 0;
        while budget > 0 {
            let Some(mut delivery) = self.pending.pop_front() else {
                break;
            };
            let take = delivery.readers.len().min(budget);
            let now: Vec<UserId> = delivery.readers.drain(..take).collect();
            for reader in now {
                self.deliver(reader, delivery.key);
            }
            budget -= take;
            done += take;
            if !delivery.readers.is_empty() {
                self.pending.push_front(delivery);
            }
        }
        done
    }

    fn posted(&mut self, author: UserId, key: Cursor) {
        let max = self.limits.max;
        keep(self.by_author.entry(author).or_default(), key, max);
        self.deliver(author, key);
        if self.celebrity(author) {
            return;
        }
        let readers: Vec<UserId> = self
            .followers
            .get(&author)
            .map(|set| set.iter().copied().collect())
            .unwrap_or_default();
        let inline = readers.len().min(self.limits.inline);
        for reader in &readers[..inline] {
            self.deliver(*reader, key);
        }
        if inline < readers.len() {
            self.pending.push_back(Delivery {
                key,
                readers: readers[inline..].to_vec(),
            });
        }
    }

    fn followed(&mut self, follower: UserId, followee: UserId) {
        self.follows.insert((follower, followee));
        self.followers.entry(followee).or_default().insert(follower);
        if self.celebrity(followee) {
            return;
        }
        let recent: Vec<Cursor> = self
            .by_author
            .get(&followee)
            .map(|set| set.iter().copied().collect())
            .unwrap_or_default();
        for key in recent {
            self.deliver(follower, key);
        }
    }

    fn gone(&mut self, user: UserId) {
        self.authors.remove(&user);
        self.follows
            .retain(|(who, whom)| *who != user && *whom != user);
        self.mutes
            .retain(|(who, whom)| *who != user && *whom != user);
        self.followers.remove(&user);
        for set in self.followers.values_mut() {
            set.remove(&user);
        }
        self.timelines.remove(&user);
        for item in self.posts.values_mut().filter(|item| item.author == user) {
            item.deleted = true;
        }
    }

    pub(super) fn effect(&mut self, change: Change) {
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
            Change::Followed { follower, followee } => self.followed(follower, followee),
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
                let item = Item {
                    author,
                    author_name,
                    text,
                    tags,
                    links,
                    likes: 0,
                    at,
                    deleted: false,
                };
                self.posts.insert(post, item);
                self.posted(author, Cursor { at, post });
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
            Change::Gone { user } => self.gone(user),
            Change::Nothing => {}
        }
    }

    pub(super) fn name_of(&self, item: &Item) -> String {
        match self.authors.get(&item.author) {
            Some(author) => author
                .display_name
                .clone()
                .unwrap_or_else(|| author.username.clone()),
            None => item.author_name.clone(),
        }
    }

    pub(super) fn visible(&self, viewer: UserId, author: UserId) -> bool {
        author == viewer
            || (self.follows.contains(&(viewer, author)) && !self.mutes.contains(&(viewer, author)))
    }

    fn shown(&self, viewer: UserId, key: &Cursor) -> Option<FeedItem> {
        let item = self.posts.get(&key.post)?;
        if item.deleted || !self.visible(viewer, item.author) {
            return None;
        }
        Some(FeedItem {
            post: key.post,
            author: item.author,
            author_name: self.name_of(item),
            text: item.text.clone(),
            likes: item.likes,
            tags: item.tags.clone(),
            links: item.links.clone(),
            at: item.at,
        })
    }

    fn collect(
        &self,
        viewer: UserId,
        line: &BTreeSet<Cursor>,
        before: Option<Cursor>,
        take: usize,
        into: &mut Vec<FeedItem>,
    ) {
        let keys: Box<dyn Iterator<Item = &Cursor>> = match before {
            Some(before) => Box::new(line.range(..before).rev()),
            None => Box::new(line.iter().rev()),
        };
        let mut found = 0;
        for key in keys {
            if found >= take {
                break;
            }
            if let Some(item) = self.shown(viewer, key) {
                into.push(item);
                found += 1;
            }
        }
    }

    pub(super) fn after(
        &self,
        viewer: UserId,
        before: Option<Cursor>,
        take: usize,
    ) -> Vec<FeedItem> {
        let mut items = Vec::new();
        if let Some(line) = self.timelines.get(&viewer) {
            self.collect(viewer, line, before, take, &mut items);
        }
        let stars: Vec<UserId> = self
            .follows
            .range((viewer, UserId(0))..=(viewer, UserId(u64::MAX)))
            .map(|(_, followee)| *followee)
            .filter(|followee| self.celebrity(*followee))
            .collect();
        for star in stars {
            if let Some(line) = self.by_author.get(&star) {
                self.collect(viewer, line, before, take, &mut items);
            }
        }
        items.sort_by(|a, b| b.at.cmp(&a.at).then_with(|| b.post.0.cmp(&a.post.0)));
        items.dedup_by_key(|item| item.post);
        items.truncate(take);
        items
    }
}
