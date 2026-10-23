use std::fmt;
use std::io::Write;
use std::sync::{Arc, Mutex, PoisonError};

use contracts::Push;

use crate::domain::{PostId, UserId};
use crate::events::Event;
use crate::shards::Shards;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Notification {
    Followed { by: UserId },
    Liked { by: UserId, post: PostId },
    Message { from: UserId },
    NewPost { author: UserId, post: PostId },
}

impl Notification {
    pub fn for_event(event: &Event) -> Option<(UserId, Notification)> {
        match *event {
            Event::Followed { who, whom } => Some((whom, Notification::Followed { by: who })),
            Event::Liked { by, post, author } if by != author => {
                Some((author, Notification::Liked { by, post }))
            }
            Event::MessageSent { from, to } => Some((to, Notification::Message { from })),
            Event::Liked { .. } | Event::NewPost { .. } => None,
        }
    }

    pub fn to_push(self, to: UserId) -> Push {
        match self {
            Notification::Followed { by } => Push::Followed { to, by },
            Notification::Liked { by, post } => Push::Liked { to, by, post },
            Notification::Message { from } => Push::Message { to, from },
            Notification::NewPost { author, post } => Push::NewPost { to, author, post },
        }
    }
}

impl fmt::Display for Notification {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Notification::Followed { by } => write!(f, "подписка от id {}", by.0),
            Notification::Liked { by, post } => {
                write!(f, "лайк от id {} на пост [{}]", by.0, post.0)
            }
            Notification::Message { from } => write!(f, "сообщение от id {}", from.0),
            Notification::NewPost { author, post } => {
                write!(f, "новый пост [{}] от id {}", post.0, author.0)
            }
        }
    }
}

pub trait Notifier: Send + Sync {
    fn notify(&self, user: UserId, notification: Notification);

    fn notify_batch(&self, batch: &[(UserId, Notification)]) {
        for &(user, notification) in batch {
            self.notify(user, notification);
        }
    }
}

#[derive(Clone, Default)]
pub struct Notifiers {
    list: Vec<Arc<dyn Notifier>>,
}

impl Notifiers {
    pub fn new() -> Notifiers {
        Notifiers::default()
    }

    pub fn add(&mut self, notifier: Arc<dyn Notifier>) {
        self.list.push(notifier);
    }

    pub fn deliver(&self, event: &Event) {
        if let Some((user, notification)) = Notification::for_event(event) {
            self.notify(user, notification);
        }
    }
}

impl Notifier for Notifiers {
    fn notify(&self, user: UserId, notification: Notification) {
        for notifier in &self.list {
            notifier.notify(user, notification);
        }
    }

    fn notify_batch(&self, batch: &[(UserId, Notification)]) {
        for notifier in &self.list {
            notifier.notify_batch(batch);
        }
    }
}

pub struct LogNotifier {
    out: Mutex<Box<dyn Write + Send>>,
}

impl LogNotifier {
    pub fn new(out: impl Write + Send + 'static) -> LogNotifier {
        LogNotifier {
            out: Mutex::new(Box::new(out)),
        }
    }
}

impl Notifier for LogNotifier {
    fn notify(&self, user: UserId, notification: Notification) {
        let mut out = self.out.lock().unwrap_or_else(PoisonError::into_inner);
        let _ = writeln!(out, "[notify] id {}: {notification}", user.0);
    }
}

#[derive(Default)]
pub struct PushQueue {
    pushes: Mutex<Vec<Push>>,
}

impl PushQueue {
    pub fn new() -> PushQueue {
        PushQueue::default()
    }

    pub fn drain(&self) -> Vec<Push> {
        let mut pushes = self.pushes.lock().unwrap_or_else(PoisonError::into_inner);
        pushes.drain(..).collect()
    }
}

impl Notifier for PushQueue {
    fn notify(&self, user: UserId, notification: Notification) {
        let mut pushes = self.pushes.lock().unwrap_or_else(PoisonError::into_inner);
        pushes.push(notification.to_push(user));
    }
}

pub struct Inbox {
    boxes: Shards<UserId, Vec<Notification>>,
}

impl Inbox {
    pub fn new(shards: usize) -> Inbox {
        Inbox {
            boxes: Shards::new(shards),
        }
    }

    pub fn inbox(&self, user: UserId) -> Vec<Notification> {
        self.boxes.get(&user).unwrap_or_default()
    }
}

impl Notifier for Inbox {
    fn notify(&self, user: UserId, notification: Notification) {
        self.boxes.with(&user, |boxes| {
            boxes.entry(user).or_default().push(notification)
        });
    }
}
