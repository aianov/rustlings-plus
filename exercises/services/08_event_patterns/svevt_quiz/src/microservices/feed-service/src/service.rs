use events::{Event, Journal, UpcastError, upcast};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Mutex;

#[derive(Debug, Clone, PartialEq)]
pub struct FeedItem {
    pub post_id: String,
    pub author_id: String,
    pub author_name: String,
    pub text: String,
    pub created_at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Consumed {
    Applied,
    Duplicate,
    Buffered,
    Ignored,
}

#[derive(Debug, PartialEq)]
pub enum FeedError {
    Upcast(UpcastError),
    BadPayload(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    pub names: BTreeMap<String, String>,
    pub follows: BTreeSet<(String, String)>,
    pub posts: Vec<(String, String, String, u64)>,
    pub waiting: usize,
}

#[derive(Default)]
struct State {
    next: HashMap<String, u64>,
    held: HashMap<String, BTreeMap<u64, Event>>,
    names: BTreeMap<String, String>,
    follows: BTreeSet<(String, String)>,
    posts: Vec<(String, String, String, u64)>,
}

#[derive(Default)]
pub struct FeedService {
    state: Mutex<State>,
}

impl FeedService {
    pub fn new() -> FeedService {
        FeedService::default()
    }

    pub fn consume(&self, event: &Event) -> Result<Consumed, FeedError> {
        todo!()
    }

    pub fn rebuild(journal: &Journal) -> Result<FeedService, FeedError> {
        todo!()
    }

    pub fn feed(&self, viewer: &str, limit: usize) -> Vec<FeedItem> {
        todo!()
    }

    pub fn followers(&self, user: &str) -> usize {
        self.state.lock().unwrap().follows.iter().filter(|(_, followee)| followee == user).count()
    }

    pub fn snapshot(&self) -> Snapshot {
        let state = self.state.lock().unwrap();
        let mut posts = state.posts.clone();
        posts.sort();
        Snapshot { names: state.names.clone(), follows: state.follows.clone(), posts, waiting: state.held.values().map(BTreeMap::len).sum() }
    }
}
