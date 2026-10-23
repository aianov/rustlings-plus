use std::collections::HashMap;
use std::io;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;

use crate::domain::{PostId, UserId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Notification {
    Followed { by: UserId },
    Liked { by: UserId, post: PostId },
}

#[derive(Debug, Clone, Copy)]
pub enum Event {
    Followed {
        who: UserId,
        whom: UserId,
    },
    Liked {
        by: UserId,
        post: PostId,
        author: UserId,
    },
}

impl Event {
    fn delivery(self) -> (UserId, Notification) {
        match self {
            Event::Followed { who, whom } => (whom, Notification::Followed { by: who }),
            Event::Liked { by, post, author } => (author, Notification::Liked { by, post }),
        }
    }
}

enum Message {
    Event(Event),
    Flush(Sender<()>),
}

type Inboxes = Arc<Mutex<HashMap<UserId, Vec<Notification>>>>;

#[derive(Clone)]
pub struct Notifier {
    sender: Sender<Message>,
    inboxes: Inboxes,
}

impl Notifier {
    pub fn start() -> io::Result<Notifier> {
        let (sender, receiver) = mpsc::channel();
        let inboxes = Inboxes::default();
        let delivered = Arc::clone(&inboxes);
        thread::Builder::new()
            .name(String::from("notify"))
            .spawn(move || deliver(receiver, delivered))?;
        Ok(Notifier { sender, inboxes })
    }

    pub fn send(&self, event: Event) {
        let _ = self.sender.send(Message::Event(event));
    }

    pub fn inbox(&self, user: UserId) -> Vec<Notification> {
        self.flush();
        let inboxes = self.inboxes.lock().unwrap_or_else(PoisonError::into_inner);
        inboxes.get(&user).cloned().unwrap_or_default()
    }

    fn flush(&self) {
        let (done, finished) = mpsc::channel();
        if self.sender.send(Message::Flush(done)).is_ok() {
            let _ = finished.recv();
        }
    }
}

fn deliver(receiver: Receiver<Message>, inboxes: Inboxes) {
    for message in receiver {
        match message {
            Message::Event(event) => {
                let (user, notification) = event.delivery();
                let mut inboxes = inboxes.lock().unwrap_or_else(PoisonError::into_inner);
                inboxes.entry(user).or_default().push(notification);
            }
            Message::Flush(done) => {
                let _ = done.send(());
            }
        }
    }
}
