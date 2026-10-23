use std::collections::HashSet;

pub const MAX_CHARS: usize = 280;
pub const COOLDOWN_SECS: u64 = 30;

pub trait Clock {
    fn now(&self) -> u64;
}

#[derive(Debug, Clone, PartialEq)]
pub struct Post {
    pub id: u32,
    pub author: String,
    pub text: String,
    pub at: u64,
}

pub trait PostStore {
    fn insert(&mut self, post: Post);
    fn all(&self) -> Vec<Post>;
    fn next_id(&self) -> u32;
}

#[derive(Default)]
pub struct VecStore {
    posts: Vec<Post>,
}

impl PostStore for VecStore {
    fn insert(&mut self, post: Post) {
        self.posts.push(post);
    }

    fn all(&self) -> Vec<Post> {
        self.posts.clone()
    }

    fn next_id(&self) -> u32 {
        self.posts.len() as u32 + 1
    }
}

#[derive(Debug, PartialEq)]
pub enum FeedError {
    EmptyAuthor,
    EmptyText,
    TooLong(usize),
    TooFast { wait: u64 },
}

fn clean(text: &str) -> String {
    text.chars().filter(|c| !c.is_control() || *c == '\n').collect::<String>().trim().to_string()
}

pub struct Feed<C: Clock, S: PostStore> {
    clock: C,
    store: S,
}

impl<C: Clock, S: PostStore> Feed<C, S> {
    pub fn new(clock: C, store: S) -> Self {
        Feed { clock, store }
    }

    pub fn publish(&mut self, author: &str, text: &str) -> Result<u32, FeedError> {
        let author = author.trim();
        if author.is_empty() {
            return Err(FeedError::EmptyAuthor);
        }
        let text = clean(text);
        if text.is_empty() {
            return Err(FeedError::EmptyText);
        }
        let chars = text.chars().count();
        if chars > MAX_CHARS {
            return Err(FeedError::TooLong(chars));
        }
        let now = self.clock.now();
        let last = self.store.all().iter().filter(|p| p.author == author).map(|p| p.at).max();
        if let Some(last) = last {
            let passed = now.saturating_sub(last);
            if passed < COOLDOWN_SECS {
                return Err(FeedError::TooFast { wait: COOLDOWN_SECS - passed });
            }
        }
        let id = self.store.next_id();
        self.store.insert(Post { id, author: author.to_string(), text, at: now });
        Ok(id)
    }

    pub fn page(&self, author: Option<&str>, before: Option<u32>, size: usize) -> Vec<Post> {
        let mut posts: Vec<Post> = self
            .store
            .all()
            .into_iter()
            .filter(|p| author.is_none_or(|a| p.author.to_lowercase() == a.to_lowercase()))
            .filter(|p| before.is_none_or(|b| p.id < b))
            .collect();
        posts.sort_by(|a, b| b.id.cmp(&a.id));
        posts.truncate(size);
        posts
    }

    pub fn summary(&self) -> String {
        let posts = self.store.all();
        let authors: HashSet<String> = posts.iter().map(|p| p.author.clone()).collect();
        format!("постов: {}, авторов: {}", posts.len(), authors.len())
    }
}
