use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

use crate::domain::{PostId, Timestamp};

pub const TRENDS_WINDOW: u64 = 24 * 60 * 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Trend {
    pub post: PostId,
    pub likes: usize,
}

#[derive(Debug, Default)]
pub struct Trends {
    likes: HashMap<PostId, Vec<Timestamp>>,
}

impl Trends {
    pub fn record(&mut self, post: PostId, at: Timestamp) {
        self.likes.entry(post).or_default().push(at);
    }

    pub fn forget(&mut self, post: PostId) {
        self.likes.remove(&post);
    }

    pub fn top(&self, now: Timestamp, window: u64, n: usize) -> Vec<Trend> {
        let from = now.saturating_sub(window);
        let mut heap = BinaryHeap::with_capacity(n + 1);
        for (post, times) in &self.likes {
            let likes = times.iter().filter(|at| **at > from && **at <= now).count();
            if likes == 0 {
                continue;
            }
            heap.push(Reverse((likes, Reverse(*post))));
            if heap.len() > n {
                heap.pop();
            }
        }
        heap.into_sorted_vec()
            .into_iter()
            .map(|Reverse((likes, Reverse(post)))| Trend { post, likes })
            .collect()
    }
}
