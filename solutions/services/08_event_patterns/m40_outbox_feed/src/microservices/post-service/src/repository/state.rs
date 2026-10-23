use std::collections::{HashMap, HashSet};

use contracts::{CommentId, Outbox, PostId, UserId};

use super::PostEntry;
use crate::common::{Comment, Link, Post};

#[derive(Default)]
pub(super) struct State {
    pub(super) posts: Vec<Post>,
    pub(super) likers: HashMap<PostId, HashSet<UserId>>,
    pub(super) likes: Vec<(PostId, u64)>,
    pub(super) comments: Vec<Comment>,
    pub(super) outbox: Outbox,
}

impl State {
    pub(super) fn index(id: PostId) -> Option<usize> {
        usize::try_from(id.0).ok()?.checked_sub(1)
    }

    pub(super) fn post_mut(&mut self, id: PostId) -> Option<&mut Post> {
        let index = State::index(id)?;
        self.posts.get_mut(index).filter(|post| !post.deleted)
    }

    pub(super) fn post(&self, id: PostId) -> Option<&Post> {
        let index = State::index(id)?;
        self.posts.get(index).filter(|post| !post.deleted)
    }

    pub(super) fn apply(&mut self, entry: PostEntry) {
        match entry {
            PostEntry::Posted {
                id,
                author,
                author_name,
                text,
                tags,
                links,
                at,
            } => self.posts.push(Post {
                id: PostId(id),
                author: UserId(author),
                author_name,
                text,
                tags,
                links: links
                    .into_iter()
                    .map(|link| Link {
                        url: link.url,
                        title: link.title,
                    })
                    .collect(),
                likes: 0,
                at,
                deleted: false,
            }),
            PostEntry::Published { through } => self.outbox.published_through(through),
            PostEntry::Liked { post, by, at } => {
                self.like(UserId(by), PostId(post), at);
            }
            PostEntry::Edited { post, text } => {
                if let Some(post) = self.post_mut(PostId(post)) {
                    post.text = text;
                }
            }
            PostEntry::Deleted { post } => {
                if let Some(post) = self.post_mut(PostId(post)) {
                    post.deleted = true;
                }
            }
            PostEntry::Commented {
                id,
                post,
                author,
                text,
            } => self.comments.push(Comment {
                id: CommentId(id),
                post: PostId(post),
                author: UserId(author),
                text,
            }),
        }
    }

    pub(super) fn like(&mut self, user: UserId, id: PostId, at: u64) -> bool {
        if !self.likers.entry(id).or_default().insert(user) {
            return false;
        }
        if let Some(post) = self.post_mut(id) {
            post.likes += 1;
        }
        self.likes.push((id, at));
        true
    }
}
