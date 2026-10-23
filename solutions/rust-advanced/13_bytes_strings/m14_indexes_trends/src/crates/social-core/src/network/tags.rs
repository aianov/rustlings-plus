use std::collections::{BTreeSet, HashMap};

use crate::domain::PostId;

#[derive(Default)]
pub struct Tags {
    index: HashMap<String, BTreeSet<PostId>>,
}

impl Tags {
    pub fn add(&mut self, post: PostId, text: &str) {
        for tag in extract(text) {
            self.index.entry(tag).or_default().insert(post);
        }
    }

    pub fn remove(&mut self, post: PostId, text: &str) {
        for tag in extract(text) {
            if let Some(posts) = self.index.get_mut(&tag) {
                posts.remove(&post);
                if posts.is_empty() {
                    self.index.remove(&tag);
                }
            }
        }
    }

    pub fn posts(&self, tag: &str) -> impl DoubleEndedIterator<Item = PostId> + '_ {
        self.index
            .get(&normalize_tag(tag))
            .into_iter()
            .flatten()
            .copied()
    }
}

fn extract(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split_whitespace()
        .filter(|word| word.starts_with('#'))
        .map(normalize_tag)
        .filter(|tag| !tag.is_empty())
}

fn normalize_tag(word: &str) -> String {
    let tag = word.trim_start_matches('#');
    let end = tag
        .char_indices()
        .find(|(_, c)| !(c.is_alphanumeric() || *c == '_'))
        .map_or(tag.len(), |(at, _)| at);
    tag[..end].to_lowercase()
}
