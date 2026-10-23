use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, PoisonError};

use contracts::UserId;
use tokio::sync::mpsc;

use super::error::ChatError;
use crate::common::Message;

pub const BACKLOG: usize = 64;

#[derive(Default)]
struct Inboxes {
    open: HashMap<UserId, Vec<mpsc::Sender<Message>>>,
    closed: bool,
}

#[derive(Default)]
pub struct Hub {
    inboxes: Mutex<Inboxes>,
}

impl Hub {
    fn lock(&self) -> MutexGuard<'_, Inboxes> {
        self.inboxes.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn subscribe(&self, user: UserId) -> Result<mpsc::Receiver<Message>, ChatError> {
        let mut inboxes = self.lock();
        if inboxes.closed {
            return Err(ChatError::Closing);
        }
        let (tx, rx) = mpsc::channel(BACKLOG);
        let own = inboxes.open.entry(user).or_default();
        own.retain(|inbox| !inbox.is_closed());
        own.push(tx);
        Ok(rx)
    }

    pub fn deliver(&self, message: &Message) -> usize {
        let mut inboxes = self.lock();
        let Some(own) = inboxes.open.get_mut(&message.to) else {
            return 0;
        };
        let before = own.len();
        own.retain(|inbox| inbox.try_send(message.clone()).is_ok());
        let delivered = own.len();
        if own.is_empty() {
            inboxes.open.remove(&message.to);
        }
        if delivered < before {
            eprintln!(
                "chat-service: у пользователя {} закрыто подписок: {}, они ушли или не успевают читать",
                message.to,
                before - delivered
            );
        }
        delivered
    }

    pub fn subscribers(&self, user: UserId) -> usize {
        self.lock().open.get(&user).map_or(0, |own| {
            own.iter().filter(|inbox| !inbox.is_closed()).count()
        })
    }

    pub fn close(&self) {
        let mut inboxes = self.lock();
        inboxes.closed = true;
        inboxes.open.clear();
    }
}
