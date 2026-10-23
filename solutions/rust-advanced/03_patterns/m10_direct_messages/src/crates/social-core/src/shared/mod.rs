use std::io;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use crate::chat::Message;
use crate::domain::{DomainError, Post, PostId, UserId, Visibility};
use crate::network::Network;
use crate::notify::{Event, Notification, Notifier};
use crate::ports::{PostStore, UserStore};

pub struct SharedNetwork<U, P> {
    network: Arc<Mutex<Network<U, P>>>,
    notifier: Notifier,
}

impl<U, P> Clone for SharedNetwork<U, P> {
    fn clone(&self) -> SharedNetwork<U, P> {
        SharedNetwork {
            network: Arc::clone(&self.network),
            notifier: self.notifier.clone(),
        }
    }
}

impl<U: UserStore, P: PostStore> SharedNetwork<U, P> {
    pub fn new(network: Network<U, P>) -> io::Result<SharedNetwork<U, P>> {
        Ok(SharedNetwork {
            network: Arc::new(Mutex::new(network)),
            notifier: Notifier::start()?,
        })
    }

    pub fn register(&self, name: &str) -> Result<UserId, DomainError> {
        self.lock().register(name)
    }

    pub fn publish(
        &self,
        author: UserId,
        text: &str,
        visibility: Visibility,
    ) -> Result<PostId, DomainError> {
        self.lock().publish(author, text, visibility)
    }

    pub fn follow(&self, who: UserId, whom: UserId) -> Result<bool, DomainError> {
        let mut network = self.lock();
        let added = network.follow(who, whom)?;
        if added {
            self.notifier.send(Event::Followed { who, whom });
        }
        Ok(added)
    }

    pub fn like(&self, user: UserId, post: PostId) -> Result<bool, DomainError> {
        let mut network = self.lock();
        let liked = network.like(user, post)?;
        let author = network.post(post).map(Post::author);
        if let Some(author) = author.filter(|author| liked && *author != user) {
            self.notifier.send(Event::Liked {
                by: user,
                post,
                author,
            });
        }
        Ok(liked)
    }

    pub fn send_message(
        &self,
        sender: UserId,
        recipient: UserId,
        text: &str,
    ) -> Result<Message, DomainError> {
        self.lock().send_message(sender, recipient, text).cloned()
    }

    pub fn read<R>(&self, f: impl FnOnce(&Network<U, P>) -> R) -> R {
        f(&self.lock())
    }

    pub fn inbox(&self, user: UserId) -> Vec<Notification> {
        self.notifier.inbox(user)
    }

    fn lock(&self) -> MutexGuard<'_, Network<U, P>> {
        self.network.lock().unwrap_or_else(PoisonError::into_inner)
    }
}
