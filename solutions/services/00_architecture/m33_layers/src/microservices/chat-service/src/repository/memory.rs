use std::collections::HashMap;

use contracts::UserId;
use journal::{Journal, JournalError};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use super::{BoxFuture, ChatRepository};
use crate::common::Message;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Line {
    pub from: u64,
    pub to: u64,
    pub text: String,
}

type Dialogs = HashMap<(UserId, UserId), Vec<Message>>;

fn pair(a: UserId, b: UserId) -> (UserId, UserId) {
    if a <= b { (a, b) } else { (b, a) }
}

fn store(dialogs: &mut Dialogs, message: Message) {
    dialogs
        .entry(pair(message.from, message.to))
        .or_default()
        .push(message);
}

pub struct MemoryChats {
    dialogs: Mutex<Dialogs>,
    journal: Journal<Line>,
}

impl MemoryChats {
    pub fn new() -> MemoryChats {
        MemoryChats::restore(Journal::memory(), Vec::new())
    }

    pub fn restore(journal: Journal<Line>, lines: Vec<Line>) -> MemoryChats {
        let mut dialogs = Dialogs::new();
        for line in lines {
            let message = Message {
                from: UserId(line.from),
                to: UserId(line.to),
                text: line.text,
            };
            store(&mut dialogs, message);
        }
        MemoryChats {
            dialogs: Mutex::new(dialogs),
            journal,
        }
    }
}

impl Default for MemoryChats {
    fn default() -> MemoryChats {
        MemoryChats::new()
    }
}

impl ChatRepository for MemoryChats {
    fn append(&self, message: Message) -> BoxFuture<'_, Result<Message, JournalError>> {
        Box::pin(async move {
            let mut dialogs = self.dialogs.lock().await;
            let line = Line {
                from: message.from.0,
                to: message.to.0,
                text: message.text.clone(),
            };
            self.journal.append(&line).await?;
            store(&mut dialogs, message.clone());
            Ok(message)
        })
    }

    fn history(&self, user: UserId, peer: UserId, limit: usize) -> BoxFuture<'_, Vec<Message>> {
        Box::pin(async move {
            let dialogs = self.dialogs.lock().await;
            match dialogs.get(&pair(user, peer)) {
                Some(dialog) => dialog[dialog.len().saturating_sub(limit)..].to_vec(),
                None => Vec::new(),
            }
        })
    }
}
