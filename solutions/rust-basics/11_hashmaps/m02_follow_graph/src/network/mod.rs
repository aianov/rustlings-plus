mod follows;
mod tags;

use std::collections::HashMap;

use crate::domain::{Post, PostId, User, UserId, Visibility, normalize_name};
use follows::Follows;
use tags::Tags;

#[derive(Default)]
pub struct Network {
    users: HashMap<UserId, User>,
    names: HashMap<String, UserId>,
    posts: Vec<Post>,
    follows: Follows,
    tags: Tags,
    next_user: u64,
    next_post: u64,
}

impl Network {
    pub fn new() -> Network {
        Network::default()
    }

    pub fn register(&mut self, name: &str) -> UserId {
        let name = normalize_name(name);
        let next = UserId(self.next_user + 1);
        let id = *self.names.entry(name.clone()).or_insert(next);
        if id == next {
            self.next_user += 1;
            self.users.insert(id, User::new(id, name));
        }
        id
    }

    pub fn user_count(&self) -> usize {
        self.users.len()
    }

    pub fn is_registered(&self, name: &str) -> bool {
        self.names.contains_key(&normalize_name(name))
    }

    pub fn publish(&mut self, author: UserId, text: &str, visibility: Visibility) -> PostId {
        self.next_post += 1;
        let id = PostId(self.next_post);
        self.tags.add(id, text);
        self.posts.push(Post::new(id, author, text.to_string(), visibility));
        id
    }

    pub fn post_count(&self) -> usize {
        self.posts.len()
    }

    pub fn like(&mut self, user: UserId, post: PostId) -> bool {
        for candidate in self.posts.iter_mut() {
            if candidate.id() == post {
                return candidate.like(user);
            }
        }
        false
    }

    pub fn likes(&self, post: PostId) -> usize {
        for candidate in &self.posts {
            if candidate.id() == post {
                return candidate.likes();
            }
        }
        0
    }

    pub fn follow(&mut self, who: UserId, whom: UserId) -> bool {
        if who == whom {
            return false;
        }
        self.follows.add(who, whom)
    }

    pub fn is_following(&self, who: UserId, whom: UserId) -> bool {
        self.follows.contains(who, whom)
    }

    pub fn following_count(&self, user: UserId) -> usize {
        self.follows.following_count(user)
    }

    pub fn followers_count(&self, user: UserId) -> usize {
        self.follows.followers_count(user)
    }

    pub fn posts_with_tag(&self, tag: &str) -> Vec<PostId> {
        self.tags.posts(tag)
    }
}
