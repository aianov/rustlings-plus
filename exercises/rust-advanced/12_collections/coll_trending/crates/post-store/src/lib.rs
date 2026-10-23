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
pub struct Store {}

impl Store {
    pub fn new() -> Store {
        Store::default()
    }

    pub fn add_post(&mut self, author: u64, ts: u64, text: &str) -> u64 {
        todo!("создать пост и обновить индексы")
    }

    pub fn get(&self, id: u64) -> Option<&Post> {
        todo!("пост по id")
    }

    pub fn edit(&mut self, id: u64, text: &str) -> bool {
        todo!("новый текст и согласованные теги")
    }

    pub fn delete(&mut self, id: u64) -> bool {
        todo!("убрать пост из всех индексов и лайков")
    }

    pub fn like(&mut self, post: u64, user: u64, ts: u64) -> Result<bool, StoreError> {
        todo!("один лайк на пользователя")
    }

    pub fn like_count(&self, post: u64) -> u32 {
        todo!("всего лайков")
    }

    pub fn author_page(&self, author: u64, cursor: Option<(u64, u64)>, n: usize) -> Page {
        todo!("страница автора от новых к старым")
    }

    pub fn with_tag(&self, tag: &str) -> Vec<u64> {
        todo!("id постов тега по возрастанию")
    }

    pub fn tags_known(&self) -> usize {
        todo!("теги, у которых есть посты")
    }

    pub fn trending(&self, now: u64, window: u64, n: usize) -> Vec<(u64, u32)> {
        todo!("топ-n постов по лайкам за окно")
    }
}
