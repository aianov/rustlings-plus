use std::collections::{BTreeSet, HashMap};
use std::ops::Bound;

use crate::domain::{Cursor, Post, UserId};

#[derive(Debug)]
pub struct AuthorPage<'a> {
    pub posts: Vec<&'a Post>,
    pub next: Option<Cursor>,
}

#[derive(Debug, Default)]
pub struct Authors {
    posts: HashMap<UserId, BTreeSet<Cursor>>,
}

impl Authors {
    pub fn add(&mut self, author: UserId, key: Cursor) {
        self.posts.entry(author).or_default().insert(key);
    }

    pub fn remove(&mut self, author: UserId, key: Cursor) {
        if let Some(keys) = self.posts.get_mut(&author) {
            keys.remove(&key);
            if keys.is_empty() {
                self.posts.remove(&author);
            }
        }
    }

    pub fn newest_first(
        &self,
        author: UserId,
        after: Option<Cursor>,
    ) -> impl Iterator<Item = Cursor> + '_ {
        let upper = after.map_or(Bound::Unbounded, Bound::Excluded);
        self.posts
            .get(&author)
            .into_iter()
            .flat_map(move |keys| keys.range((Bound::Unbounded, upper)).rev().copied())
    }
}
