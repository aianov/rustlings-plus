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

    /// Публикует пост. Возвращает номер нового поста.
    ///
    /// ```
    /// use feedlib::{Clock, Feed, FeedError, VecStore};
    ///
    /// struct Noon;
    /// impl Clock for Noon {
    ///     fn now(&self) -> u64 {
    ///         43_200
    ///     }
    /// }
    ///
    /// let mut feed = Feed::new(Noon, VecStore::default());
    /// assert_eq!(feed.publish("ann", "привет"), Ok(1));
    /// assert_eq!(feed.publish("ann", "ещё раз"), Err(FeedError::TooFast { wait: 30 }));
    /// ```
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

    /// Страница ленты: сначала новые, `before` исключает посты с номером не меньше указанного.
    ///
    /// ```
    /// use feedlib::{Clock, Feed, VecStore};
    ///
    /// struct Noon;
    /// impl Clock for Noon {
    ///     fn now(&self) -> u64 {
    ///         43_200
    ///     }
    /// }
    ///
    /// let mut feed = Feed::new(Noon, VecStore::default());
    /// feed.publish("ann", "первый").unwrap();
    /// feed.publish("bob", "второй").unwrap();
    /// let ids: Vec<u32> = feed.page(None, None, 10).iter().map(|p| p.id).collect();
    /// assert_eq!(ids, vec![2, 1]);
    /// ```
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    struct Clk(Cell<u64>);

    impl Clock for Clk {
        fn now(&self) -> u64 {
            self.0.get()
        }
    }

    fn feed() -> Feed<Clk, VecStore> {
        Feed::new(Clk(Cell::new(1_000)), VecStore::default())
    }

    #[test]
    fn clean_drops_control_characters_but_keeps_newlines() {
        assert_eq!(clean("a\u{7}b\nc\td"), "ab\ncd");
    }

    #[test]
    fn clean_trims_the_edges() {
        assert_eq!(clean("  hi \n"), "hi");
        assert_eq!(clean(" \t "), "");
    }

    #[test]
    fn limit_counts_characters_not_bytes() {
        let mut feed = feed();
        assert!(feed.publish("ann", &"я".repeat(MAX_CHARS)).is_ok(), "280 кириллических букв это 560 байт, но допустимо");
    }

    #[test]
    fn longer_than_limit_reports_the_length() {
        let mut feed = feed();
        assert_eq!(feed.publish("ann", &"ф".repeat(MAX_CHARS + 1)), Err(FeedError::TooLong(MAX_CHARS + 1)));
    }

    #[test]
    fn blank_author_and_blank_text_are_rejected() {
        let mut feed = feed();
        assert_eq!(feed.publish("  ", "текст"), Err(FeedError::EmptyAuthor));
        assert_eq!(feed.publish("ann", " \u{7} "), Err(FeedError::EmptyText));
    }
}
