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

enum Change {
    Registered { user: String, name: String },
    Renamed { user: String, name: String },
    Followed { follower: String, followee: String },
    Posted { post_id: String, author: String, text: String, at: u64 },
}

#[derive(Default)]
struct State {
    next: HashMap<String, u64>,
    held: HashMap<String, BTreeMap<u64, Change>>,
    names: BTreeMap<String, String>,
    follows: BTreeSet<(String, String)>,
    posts: Vec<(String, String, String, u64)>,
}

#[derive(Default)]
pub struct FeedService {
    state: Mutex<State>,
}

fn field(payload: &Value, key: &str, kind: &str) -> Result<String, FeedError> {
    payload.get(key).and_then(Value::as_str).map(str::to_string).ok_or_else(|| FeedError::BadPayload(format!("{kind}: нет поля {key}")))
}

fn parse(event: &Event) -> Result<Option<Change>, FeedError> {
    let (p, kind) = (&event.payload, event.kind.as_str());
    let change = match kind {
        "user.registered" => Change::Registered { user: field(p, "user_id", kind)?, name: field(p, "name", kind)? },
        "user.profile_changed" => {
            let name = format!("{} {}", field(p, "first_name", kind)?, field(p, "last_name", kind)?);
            Change::Renamed { user: event.aggregate_id.clone(), name: name.trim().to_string() }
        }
        "user.followed" => Change::Followed { follower: field(p, "follower_id", kind)?, followee: field(p, "followee_id", kind)? },
        "post.created" => Change::Posted { post_id: field(p, "post_id", kind)?, author: field(p, "author_id", kind)?, text: field(p, "text", kind)?, at: event.occurred_at },
        _ => return Ok(None),
    };
    Ok(Some(change))
}

impl State {
    fn apply(&mut self, change: Change) {
        match change {
            Change::Registered { user, name } | Change::Renamed { user, name } => {
                self.names.insert(user, name);
            }
            Change::Followed { follower, followee } => {
                self.follows.insert((follower, followee));
            }
            Change::Posted { post_id, author, text, at } => self.posts.push((post_id, author, text, at)),
        }
    }
}

impl FeedService {
    pub fn new() -> FeedService {
        FeedService::default()
    }

    pub fn consume(&self, event: &Event) -> Result<Consumed, FeedError> {
        let event = upcast(event.clone()).map_err(FeedError::Upcast)?;
        let Some(change) = parse(&event)? else {
            return Ok(Consumed::Ignored);
        };
        let mut state = self.state.lock().unwrap();
        let expected = *state.next.get(&event.aggregate_id).unwrap_or(&1);
        if event.version < expected {
            return Ok(Consumed::Duplicate);
        }
        if event.version > expected {
            let held = state.held.entry(event.aggregate_id.clone()).or_default();
            if held.contains_key(&event.version) {
                return Ok(Consumed::Duplicate);
            }
            held.insert(event.version, change);
            return Ok(Consumed::Buffered);
        }
        state.apply(change);
        let mut next = expected + 1;
        while let Some(following) = state.held.get_mut(&event.aggregate_id).and_then(|held| held.remove(&next)) {
            state.apply(following);
            next += 1;
        }
        state.next.insert(event.aggregate_id.clone(), next);
        Ok(Consumed::Applied)
    }

    pub fn rebuild(journal: &Journal) -> Result<FeedService, FeedError> {
        let service = FeedService::new();
        for (_, event) in journal.read_from(1) {
            service.consume(&event)?;
        }
        Ok(service)
    }

    pub fn feed(&self, viewer: &str, limit: usize) -> Vec<FeedItem> {
        let state = self.state.lock().unwrap();
        let mut items: Vec<FeedItem> = state
            .posts
            .iter()
            .filter(|(_, author, _, _)| author == viewer || state.follows.contains(&(viewer.to_string(), author.clone())))
            .map(|(id, author, text, at)| FeedItem {
                post_id: id.clone(),
                author_id: author.clone(),
                author_name: state.names.get(author).cloned().unwrap_or_else(|| author.clone()),
                text: text.clone(),
                created_at: *at,
            })
            .collect();
        items.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| b.post_id.cmp(&a.post_id)));
        items.truncate(limit);
        items
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
