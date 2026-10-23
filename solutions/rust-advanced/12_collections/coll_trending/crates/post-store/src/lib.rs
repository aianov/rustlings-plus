use std::cmp::Reverse;
use std::collections::{BTreeSet, BinaryHeap, HashMap, HashSet};
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub struct Post {
    pub id: u64,
    pub author: u64,
    pub ts: u64,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreError {
    NoSuchPost(u64),
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoreError::NoSuchPost(id) => write!(f, "post {id} does not exist"),
        }
    }
}

impl std::error::Error for StoreError {}

#[derive(Debug, PartialEq)]
pub struct Page {
    pub posts: Vec<u64>,
    pub next: Option<(u64, u64)>,
}

pub fn hashtags(text: &str) -> HashSet<String> {
    text.split_whitespace().filter_map(|w| w.strip_prefix('#')).filter(|t| !t.is_empty()).map(str::to_lowercase).collect()
}

#[derive(Default)]
pub struct Store {
    next_id: u64,
    posts: HashMap<u64, Post>,
    by_author: HashMap<u64, BTreeSet<(u64, u64)>>,
    by_tag: HashMap<String, BTreeSet<u64>>,
    likes: HashMap<u64, HashMap<u64, u64>>,
}

impl Store {
    pub fn new() -> Store {
        Store::default()
    }

    pub fn add_post(&mut self, author: u64, ts: u64, text: &str) -> u64 {
        self.next_id += 1;
        let id = self.next_id;
        self.index(id, author, ts, text);
        self.posts.insert(id, Post { id, author, ts, text: text.to_string() });
        id
    }

    pub fn get(&self, id: u64) -> Option<&Post> {
        self.posts.get(&id)
    }

    pub fn edit(&mut self, id: u64, text: &str) -> bool {
        let Some(post) = self.posts.get(&id) else {
            return false;
        };
        let old = post.text.clone();
        self.unindex_tags(id, &old);
        self.index_tags(id, text);
        if let Some(post) = self.posts.get_mut(&id) {
            post.text = text.to_string();
        }
        true
    }

    pub fn delete(&mut self, id: u64) -> bool {
        let Some(post) = self.posts.remove(&id) else {
            return false;
        };
        if let Some(set) = self.by_author.get_mut(&post.author) {
            set.remove(&(post.ts, id));
            if set.is_empty() {
                self.by_author.remove(&post.author);
            }
        }
        self.unindex_tags(id, &post.text);
        self.likes.remove(&id);
        true
    }

    pub fn like(&mut self, post: u64, user: u64, ts: u64) -> Result<bool, StoreError> {
        if !self.posts.contains_key(&post) {
            return Err(StoreError::NoSuchPost(post));
        }
        let fresh = self.likes.entry(post).or_default().insert(user, ts).is_none();
        Ok(fresh)
    }

    pub fn like_count(&self, post: u64) -> u32 {
        self.likes.get(&post).map_or(0, |l| l.len() as u32)
    }

    pub fn author_page(&self, author: u64, cursor: Option<(u64, u64)>, n: usize) -> Page {
        let Some(keys) = self.by_author.get(&author) else {
            return Page { posts: Vec::new(), next: None };
        };
        let mut page: Vec<(u64, u64)> = match cursor {
            Some(c) => keys.range(..c).rev().take(n + 1).copied().collect(),
            None => keys.iter().rev().take(n + 1).copied().collect(),
        };
        let more = page.len() > n;
        page.truncate(n);
        let next = if more { page.last().copied() } else { None };
        Page { posts: page.into_iter().map(|(_, id)| id).collect(), next }
    }

    pub fn with_tag(&self, tag: &str) -> Vec<u64> {
        self.by_tag.get(tag).map(|ids| ids.iter().copied().collect()).unwrap_or_default()
    }

    pub fn tags_known(&self) -> usize {
        self.by_tag.len()
    }

    pub fn trending(&self, now: u64, window: u64, n: usize) -> Vec<(u64, u32)> {
        let from = now.saturating_sub(window);
        let mut heap: BinaryHeap<Reverse<(u32, Reverse<u64>)>> = BinaryHeap::new();
        for (post, likes) in &self.likes {
            let recent = likes.values().filter(|ts| **ts > from && **ts <= now).count() as u32;
            if recent == 0 {
                continue;
            }
            heap.push(Reverse((recent, Reverse(*post))));
            if heap.len() > n {
                heap.pop();
            }
        }
        heap.into_sorted_vec().into_iter().map(|Reverse((count, Reverse(post)))| (post, count)).collect()
    }

    fn index(&mut self, id: u64, author: u64, ts: u64, text: &str) {
        self.by_author.entry(author).or_default().insert((ts, id));
        self.index_tags(id, text);
    }

    fn index_tags(&mut self, id: u64, text: &str) {
        for tag in hashtags(text) {
            self.by_tag.entry(tag).or_default().insert(id);
        }
    }

    fn unindex_tags(&mut self, id: u64, text: &str) {
        for tag in hashtags(text) {
            if let Some(ids) = self.by_tag.get_mut(&tag) {
                ids.remove(&id);
                if ids.is_empty() {
                    self.by_tag.remove(&tag);
                }
            }
        }
    }
}
