mod follows;
mod tags;

use std::collections::HashMap;

use crate::domain::{
    DomainError, Post, PostId, User, UserId, Visibility, normalize_name, validate_name,
    validate_text,
};
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

    pub fn register(&mut self, name: &str) -> Result<UserId, DomainError> {
        let name = validate_name(name)?;
        if self.names.contains_key(&name) {
            return Err(DomainError::NameTaken(name));
        }
        self.next_user += 1;
        let id = UserId(self.next_user);
        self.names.insert(name.clone(), id);
        self.users.insert(id, User::new(id, name));
        Ok(id)
    }

    pub fn user_count(&self) -> usize {
        self.users.len()
    }

    pub fn is_registered(&self, name: &str) -> bool {
        self.names.contains_key(&normalize_name(name))
    }

    pub fn user(&self, id: UserId) -> Option<&User> {
        self.users.get(&id)
    }

    pub fn find_user(&self, name: &str) -> Option<&User> {
        let id = self.names.get(&normalize_name(name))?;
        self.users.get(id)
    }

    pub fn publish(
        &mut self,
        author: UserId,
        text: &str,
        visibility: Visibility,
    ) -> Result<PostId, DomainError> {
        self.ensure_user(author)?;
        let text = validate_text(text)?;
        self.next_post += 1;
        let id = PostId(self.next_post);
        self.tags.add(id, &text);
        self.posts.push(Post::new(id, author, text, visibility));
        Ok(id)
    }

    pub fn post_count(&self) -> usize {
        self.posts.len()
    }

    pub fn post(&self, id: PostId) -> Option<&Post> {
        self.posts.iter().find(|post| post.id() == id)
    }

    pub fn last_post(&self, author: UserId) -> Option<&Post> {
        self.posts.iter().rev().find(|post| post.author() == author)
    }

    pub fn like(&mut self, user: UserId, post: PostId) -> Result<bool, DomainError> {
        self.ensure_user(user)?;
        let post = self
            .posts
            .iter_mut()
            .find(|candidate| candidate.id() == post)
            .ok_or(DomainError::UnknownPost(post))?;
        Ok(post.like(user))
    }

    pub fn likes(&self, post: PostId) -> usize {
        match self.post(post) {
            Some(post) => post.likes(),
            None => 0,
        }
    }

    pub fn follow(&mut self, who: UserId, whom: UserId) -> Result<bool, DomainError> {
        self.ensure_user(who)?;
        self.ensure_user(whom)?;
        if who == whom {
            return Err(DomainError::SelfFollow);
        }
        Ok(self.follows.add(who, whom))
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

    fn ensure_user(&self, id: UserId) -> Result<(), DomainError> {
        if self.users.contains_key(&id) {
            Ok(())
        } else {
            Err(DomainError::UnknownUser(id))
        }
    }
}
