use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, PoisonError};

use contracts::UserId;
use tokio::sync::OnceCell;

use crate::common::Post;

pub const KEPT_KEYS: usize = 10_000;

pub const MAX_KEY: usize = 64;

type Key = (UserId, String);

#[derive(Default)]
struct Cells {
    cells: HashMap<Key, Arc<OnceCell<Post>>>,
    order: VecDeque<Key>,
}

#[derive(Default)]
pub struct Idempotency {
    cells: Mutex<Cells>,
}

pub fn good_key(key: &str) -> bool {
    !key.is_empty() && key.len() <= MAX_KEY && key.bytes().all(|byte| byte.is_ascii_graphic())
}

impl Idempotency {
    pub fn cell(&self, author: UserId, key: &str) -> Arc<OnceCell<Post>> {
        let mut kept = self.cells.lock().unwrap_or_else(PoisonError::into_inner);
        let id = (author, key.to_string());
        if let Some(cell) = kept.cells.get(&id) {
            return Arc::clone(cell);
        }
        let cell = Arc::new(OnceCell::new());
        kept.cells.insert(id.clone(), Arc::clone(&cell));
        kept.order.push_back(id);
        while kept.order.len() > KEPT_KEYS {
            if let Some(old) = kept.order.pop_front() {
                kept.cells.remove(&old);
            }
        }
        cell
    }

    pub fn len(&self) -> usize {
        self.cells
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .cells
            .len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
