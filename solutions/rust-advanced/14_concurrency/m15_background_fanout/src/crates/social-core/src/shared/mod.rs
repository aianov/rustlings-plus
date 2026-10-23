mod fanout;
mod profiles;

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use crate::chat::Message;
use crate::domain::{CommentId, DomainError, PostId, User, UserId, Visibility};
use crate::error::Error;
use crate::events::{Event, SubId, Topic};
use crate::network::Network;
use crate::notify::{Inbox, Notification, Notifiers};
use crate::ports::{PostStore, UserStore};
use fanout::{Counters, Fanout};
use profiles::Profiles;

pub use fanout::{FANOUT_WORKERS, Stats};

const NOTIFIED: [Topic; 3] = [Topic::Followed, Topic::Liked, Topic::MessageSent];

pub struct SharedNetwork<U, P> {
    network: Arc<Mutex<Network<U, P>>>,
    inbox: Arc<Inbox>,
    fanout: Arc<Fanout>,
    profiles: Arc<Profiles>,
}

impl<U, P> Clone for SharedNetwork<U, P> {
    fn clone(&self) -> SharedNetwork<U, P> {
        SharedNetwork {
            network: Arc::clone(&self.network),
            inbox: Arc::clone(&self.inbox),
            fanout: Arc::clone(&self.fanout),
            profiles: Arc::clone(&self.profiles),
        }
    }
}

impl<U: UserStore, P: PostStore> SharedNetwork<U, P> {
    pub fn new(network: Network<U, P>) -> crate::Result<SharedNetwork<U, P>> {
        SharedNetwork::with_notifiers(network, Notifiers::new())
    }

    pub fn with_notifiers(
        mut network: Network<U, P>,
        mut notifiers: Notifiers,
    ) -> crate::Result<SharedNetwork<U, P>> {
        let inbox = Arc::new(Inbox::start().map_err(Error::Start)?);
        notifiers.add(inbox.clone());
        let counters = Arc::new(Counters::default());
        notifiers.add(counters.clone());
        for topic in NOTIFIED {
            let notifiers = notifiers.clone();
            network.subscribe(topic, move |event| notifiers.deliver(event));
        }
        Ok(SharedNetwork {
            network: Arc::new(Mutex::new(network)),
            inbox,
            fanout: Arc::new(Fanout::start(notifiers, counters)?),
            profiles: Arc::new(Profiles::default()),
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
        let (post, audience) = {
            let mut network = self.lock();
            let post = network.publish(author, text, visibility)?;
            (post, network.audience(post))
        };
        self.fanout.published(author, post, audience);
        Ok(post)
    }

    pub fn follow(&self, who: UserId, whom: UserId) -> Result<bool, DomainError> {
        self.lock().follow(who, whom)
    }

    pub fn like(&self, user: UserId, post: PostId) -> Result<bool, DomainError> {
        self.lock().like(user, post)
    }

    pub fn mute(&self, who: UserId, whom: UserId) -> Result<bool, DomainError> {
        self.lock().mute(who, whom)
    }

    pub fn delete_post(&self, who: UserId, post: PostId) -> Result<bool, DomainError> {
        self.lock().delete_post(who, post)
    }

    pub fn comment(
        &self,
        author: UserId,
        post: PostId,
        text: &str,
    ) -> Result<CommentId, DomainError> {
        self.lock().comment(author, post, text)
    }

    pub fn reply(
        &self,
        author: UserId,
        parent: CommentId,
        text: &str,
    ) -> Result<CommentId, DomainError> {
        self.lock().reply(author, parent, text)
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
        self.flush();
        self.inbox.inbox(user)
    }

    pub fn flush(&self) {
        self.fanout.flush();
    }

    pub fn stats(&self) -> Stats {
        self.fanout.stats()
    }

    pub fn profile(&self, id: UserId) -> Option<Arc<User>> {
        if let Some(user) = self.profiles.get(id) {
            return Some(user);
        }
        let user = self.read(|network| network.user(id).cloned())?;
        Some(self.profiles.keep(user))
    }

    fn lock(&self) -> MutexGuard<'_, Network<U, P>> {
        self.network.lock().unwrap_or_else(PoisonError::into_inner)
    }
}
