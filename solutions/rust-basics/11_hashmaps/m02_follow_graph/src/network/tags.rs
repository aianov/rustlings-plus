use std::collections::HashMap;

use crate::domain::PostId;

#[derive(Default)]
pub struct Tags {
    index: HashMap<String, Vec<PostId>>,
}

impl Tags {
    pub fn add(&mut self, post: PostId, text: &str) {
        for tag in extract(text) {
            self.index.entry(tag).or_default().push(post);
        }
    }

    pub fn posts(&self, tag: &str) -> Vec<PostId> {
        match self.index.get(&normalize_tag(tag)) {
            Some(posts) => posts.clone(),
            None => Vec::new(),
        }
    }
}

fn extract(text: &str) -> Vec<String> {
    let mut tags = Vec::new();
    for word in text.split_whitespace() {
        if !word.starts_with('#') {
            continue;
        }
        let tag = normalize_tag(word);
        if !tag.is_empty() && !tags.contains(&tag) {
            tags.push(tag);
        }
    }
    tags
}

fn normalize_tag(word: &str) -> String {
    let mut tag = String::new();
    for c in word.trim_start_matches('#').chars() {
        if !(c.is_alphanumeric() || c == '_') {
            break;
        }
        tag.push(c);
    }
    tag.to_lowercase()
}
