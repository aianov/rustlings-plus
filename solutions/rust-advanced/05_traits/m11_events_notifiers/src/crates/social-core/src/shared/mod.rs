use std::io;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use crate::chat::Message;
use crate::domain::{DomainError, PostId, UserId, Visibility};
use crate::events::{Event, SubId, Topic};
use crate::network::Network;
use crate::notify::{Inbox, Notification, Notifiers};
use crate::ports::{PostStore, UserStore};

const NOTIFIED: [Topic; 3] = [Topic::Followed, Topic::Liked, Topic::MessageSent];

pub struct SharedNetwork<U, P> {
    network: Arc<Mutex<Network<U, P>>>,
    inbox: Arc<Inbox>,
}

impl<U, P> Clone for SharedNetwork<U, P> {
    fn clone(&self) -> SharedNetwork<U, P> {
        SharedNetwork {
            network: Arc::clone(&self.network),
            inbox: Arc::clone(&self.inbox),
        }
    }
}

impl<U: UserStore, P: PostStore> SharedNetwork<U, P> {
    pub fn new(network: Network<U, P>) -> io::Result<SharedNetwork<U, P>> {
        SharedNetwork::with_notifiers(network, Notifiers::new())
    }

    pub fn with_notifiers(
        mut network: Network<U, P>,
        mut notifiers: Notifiers,
    ) -> io::Result<SharedNetwork<U, P>> {
        let inbox = Arc::new(Inbox::start()?);
        notifiers.add(inbox.clone());
        for topic in NOTIFIED {
            let notifiers = notifiers.clone();
            network.subscribe(topic, move |event| notifiers.deliver(event));
        }
        Ok(SharedNetwork {
            network: Arc::new(Mutex::new(network)),
            inbox,
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
        self.lock().follow(who, whom)
    }

    pub fn like(&self, user: UserId, post: PostId) -> Result<bool, DomainError> {
        self.lock().like(user, post)
    }

    pub fn send_message(
        &self,
        sender: UserId,
        recipient: UserId,
        text: &str,
    ) -> Result<Message, DomainError> {
        self.lock().send_message(sender, recipient, text).cloned()
    }

    pub fn subscribe(&self, topic: Topic, handler: impl FnMut(&Event) + Send + 'static) -> SubId {
        self.lock().subscribe(topic, handler)
    }

    pub fn unsubscribe(&self, id: SubId) -> bool {
        self.lock().unsubscribe(id)
    }

    pub fn read<R>(&self, f: impl FnOnce(&Network<U, P>) -> R) -> R {
        f(&self.lock())
    }

    pub fn inbox(&self, user: UserId) -> Vec<Notification> {
        self.inbox.inbox(user)
    }

    fn lock(&self) -> MutexGuard<'_, Network<U, P>> {
        self.network.lock().unwrap_or_else(PoisonError::into_inner)
    }
}
