use std::collections::HashSet;

use contracts::{PostId, UserId};
use tokio::sync::broadcast;
use tokio::sync::broadcast::error::RecvError;

use crate::common::Post;

pub struct FeedWatch {
    history: Vec<Post>,
    authors: HashSet<UserId>,
    newest: Option<PostId>,
    live: broadcast::Receiver<Post>,
}

impl FeedWatch {
    pub(super) fn new(
        history: Vec<Post>,
        authors: Vec<UserId>,
        live: broadcast::Receiver<Post>,
    ) -> FeedWatch {
        let newest = history.first().map(|post| post.id);
        FeedWatch {
            history,
            authors: authors.into_iter().collect(),
            newest,
            live,
        }
    }

    pub fn history(&mut self) -> Vec<Post> {
        std::mem::take(&mut self.history)
    }

    pub async fn next(&mut self) -> Option<Post> {
        loop {
            match self.live.recv().await {
                Ok(post) => {
                    let seen = self.newest.is_some_and(|newest| post.id <= newest);
                    if !seen && self.authors.contains(&post.author) {
                        self.newest = Some(post.id);
                        return Some(post);
                    }
                }
                Err(RecvError::Lagged(..)) => {}
                Err(RecvError::Closed) => return None,
            }
        }
    }
}
