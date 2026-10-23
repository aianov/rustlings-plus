mod authors;
mod comments;
mod feed;
mod follows;
mod mutes;
mod posts;
mod tags;

use crate::chat::{Chats, Message};
use crate::comments::Comments;
use crate::domain::{DomainError, PostId, User, UserId, normalize_name, validate_name};
use crate::events::{Event, EventBus, SubId, Topic};
use crate::ports::{PostStore, UserStore};
use crate::trends::Trends;
use authors::Authors;
use follows::Follows;
use mutes::Mutes;
use tags::Tags;

pub use authors::AuthorPage;

pub struct Network<U, P> {
    users: U,
    posts: P,
    follows: Follows,
    mutes: Mutes,
    authors: Authors,
    tags: Tags,
    trends: Trends,
    comments: Comments,
    chats: Chats,
    events: EventBus,
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
            mutes: Mutes::default(),
            authors: Authors::default(),
            tags: Tags::default(),
            trends: Trends::default(),
            comments: Comments::default(),
            chats: Chats::default(),
            events: EventBus::default(),
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

    pub fn follow(&mut self, who: UserId, whom: UserId) -> Result<bool, DomainError> {
        self.ensure_user(who)?;
        self.ensure_user(whom)?;
        if who == whom {
            return Err(DomainError::SelfFollow);
        }
        let added = self.follows.add(who, whom);
        if added {
            self.events.publish(&Event::Followed { who, whom });
        }
        Ok(added)
    }

    pub fn mute(&mut self, who: UserId, whom: UserId) -> Result<bool, DomainError> {
        self.ensure_user(who)?;
        self.ensure_user(whom)?;
        if who == whom {
            return Err(DomainError::SelfMute);
        }
        Ok(self.mutes.add(who, whom))
    }

    pub fn is_muted(&self, who: UserId, whom: UserId) -> bool {
        self.mutes.contains(who, whom)
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

    pub fn send_message(
        &mut self,
        sender: UserId,
        recipient: UserId,
        text: &str,
    ) -> Result<&Message, DomainError> {
        self.ensure_user(sender)?;
        self.ensure_user(recipient)?;
        let message = self.chats.send(sender, recipient, text)?;
        self.events.publish(&Event::MessageSent {
            from: sender,
            to: recipient,
        });
        Ok(message)
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

    pub fn subscribe(
        &mut self,
        topic: Topic,
        handler: impl FnMut(&Event) + Send + 'static,
    ) -> SubId {
        self.events.subscribe(topic, handler)
    }

    pub fn unsubscribe(&mut self, id: SubId) -> bool {
        self.events.unsubscribe(id)
    }

    fn ensure_user(&self, id: UserId) -> Result<(), DomainError> {
        if self.users.contains(id) {
            Ok(())
        } else {
            Err(DomainError::UnknownUser(id))
        }
    }

    fn ensure_visible(&self, viewer: UserId, post: PostId) -> Result<(), DomainError> {
        let visible = self.posts.get(post).is_some_and(|target| {
            let follows = self.follows.contains(viewer, target.author());
            !target.is_deleted() && target.can_view(viewer, follows)
        });
        if visible {
            Ok(())
        } else {
            Err(DomainError::UnknownPost(post))
        }
    }
}
