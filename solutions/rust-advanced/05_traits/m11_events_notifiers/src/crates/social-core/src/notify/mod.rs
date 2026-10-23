use std::collections::HashMap;
use std::fmt;
use std::io::{self, Write};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;

use crate::domain::{PostId, UserId};
use crate::events::Event;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Notification {
    Followed { by: UserId },
    Liked { by: UserId, post: PostId },
    Message { from: UserId },
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
}

impl fmt::Display for Notification {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Notification::Followed { by } => write!(f, "подписка от id {}", by.0),
            Notification::Liked { by, post } => {
                write!(f, "лайк от id {} на пост [{}]", by.0, post.0)
            }
            Notification::Message { from } => write!(f, "сообщение от id {}", from.0),
        }
    }
}

pub trait Notifier: Send + Sync {
    fn notify(&self, user: UserId, notification: Notification);
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
            for notifier in &self.list {
                notifier.notify(user, notification);
            }
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

enum Envelope {
    Deliver(UserId, Notification),
    Flush(Sender<()>),
}

type Inboxes = Arc<Mutex<HashMap<UserId, Vec<Notification>>>>;

pub struct Inbox {
    sender: Sender<Envelope>,
    inboxes: Inboxes,
}

impl Inbox {
    pub fn start() -> io::Result<Inbox> {
        let (sender, receiver) = mpsc::channel();
        let inboxes = Inboxes::default();
        let delivered = Arc::clone(&inboxes);
        thread::Builder::new()
            .name(String::from("notify"))
            .spawn(move || deliver(receiver, delivered))?;
        Ok(Inbox { sender, inboxes })
    }

    pub fn inbox(&self, user: UserId) -> Vec<Notification> {
        self.flush();
        let inboxes = self.inboxes.lock().unwrap_or_else(PoisonError::into_inner);
        inboxes.get(&user).cloned().unwrap_or_default()
    }

    fn flush(&self) {
        let (done, finished) = mpsc::channel();
        if self.sender.send(Envelope::Flush(done)).is_ok() {
            let _ = finished.recv();
        }
    }
}

impl Notifier for Inbox {
    fn notify(&self, user: UserId, notification: Notification) {
        let _ = self.sender.send(Envelope::Deliver(user, notification));
    }
}

fn deliver(receiver: Receiver<Envelope>, inboxes: Inboxes) {
    for message in receiver {
        match message {
            Envelope::Deliver(user, notification) => {
                let mut inboxes = inboxes.lock().unwrap_or_else(PoisonError::into_inner);
                inboxes.entry(user).or_default().push(notification);
            }
            Envelope::Flush(done) => {
                let _ = done.send(());
            }
        }
    }
}
