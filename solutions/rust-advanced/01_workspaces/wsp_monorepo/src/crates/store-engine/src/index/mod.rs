use std::collections::HashMap;

use contracts::UserId;

#[derive(Debug, Default)]
pub struct AuthorIndex {
    positions: HashMap<u64, Vec<usize>>,
}

impl AuthorIndex {
    pub fn record(&mut self, author: UserId, position: usize) {
        self.positions.entry(author.0).or_default().push(position);
    }

    pub fn positions(&self, author: UserId) -> &[usize] {
        self.positions.get(&author.0).map_or(&[], Vec::as_slice)
    }

    pub fn authors(&self) -> usize {
        self.positions.len()
    }
}
