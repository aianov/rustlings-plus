mod feed;
mod follows;
mod tags;

use crate::chat::{Chats, Message};
use crate::domain::{
    DomainError, Post, PostId, User, UserId, Visibility, normalize_name, validate_name,
    validate_text,
};
use crate::ports::{PostStore, UserStore};
use follows::Follows;
use tags::Tags;

pub struct Network<U, P> {
    users: U,
    posts: P,
    follows: Follows,
    tags: Tags,
    chats: Chats,
}

impl<U: UserStore + Default, P: PostStore + Default> Network<U, P> {
    pub fn new() -> Network<U, P> {
        Network::with_stores(U::default(), P::default())
    }
}

impl<U: UserStore + Default, P: PostStore + Default> Default for Network<U, P> {
    fn default() -> Network<U, P> {
        Network::new()
    }
}

impl<U: UserStore, P: PostStore> Network<U, P> {
    pub fn with_stores(users: U, posts: P) -> Network<U, P> {
        Network {
            users,
            posts,
            follows: Follows::default(),
            tags: Tags::default(),
            chats: Chats::default(),
        }
    }

    pub fn register(&mut self, name: &str) -> Result<UserId, DomainError> {
        let name = validate_name(name)?;
        if self.users.is_taken(&name) {
            return Err(DomainError::NameTaken(name));
        }
        let id = self.users.next_id();
        self.users.insert(User::new(id, name));
        Ok(id)
    }

    pub fn user_count(&self) -> usize {
        self.users.count()
    }

    pub fn is_registered(&self, name: &str) -> bool {
        self.users.is_taken(&normalize_name(name))
    }

    pub fn user(&self, id: UserId) -> Option<&User> {
        self.users.get(id)
    }

    pub fn find_user(&self, name: &str) -> Option<&User> {
        self.users.find_by_name(&normalize_name(name))
    }

    pub fn publish(
        &mut self,
        author: UserId,
        text: &str,
        visibility: Visibility,
    ) -> Result<PostId, DomainError> {
        self.ensure_user(author)?;
        let text = validate_text(text)?;
        let id = self.posts.next_id();
        self.tags.add(id, &text);
        self.posts.insert(Post::new(id, author, text, visibility));
        Ok(id)
    }

    pub fn post_count(&self) -> usize {
        self.posts.count()
    }

    pub fn post(&self, id: PostId) -> Option<&Post> {
        self.posts.get(id)
    }

    pub fn last_post(&self, author: UserId) -> Option<&Post> {
        self.posts.last_by(author)
    }

    pub fn like(&mut self, user: UserId, post: PostId) -> Result<bool, DomainError> {
        self.ensure_user(user)?;
        let post = self
            .posts
            .get_mut(post)
            .ok_or(DomainError::UnknownPost(post))?;
        Ok(post.like(user))
    }

    pub fn likes(&self, post: PostId) -> usize {
        match self.posts.get(post) {
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
        self.tags.posts(tag).to_vec()
    }

    pub fn send_message(
        &mut self,
        sender: UserId,
        recipient: UserId,
        text: &str,
    ) -> Result<&Message, DomainError> {
        self.ensure_user(sender)?;
        self.ensure_user(recipient)?;
        self.chats.send(sender, recipient, text)
    }

    pub fn history(
        &self,
        user: UserId,
        peer: UserId,
        limit: usize,
    ) -> Result<&[Message], DomainError> {
        self.ensure_user(user)?;
        self.ensure_user(peer)?;
        Ok(self.chats.history(user, peer, limit))
    }

    fn ensure_user(&self, id: UserId) -> Result<(), DomainError> {
        if self.users.contains(id) {
            Ok(())
        } else {
            Err(DomainError::UnknownUser(id))
        }
    }
}
