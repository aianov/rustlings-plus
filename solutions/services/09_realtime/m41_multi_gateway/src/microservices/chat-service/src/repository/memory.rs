use std::collections::HashMap;
use std::sync::Arc;

use contracts::{Cause, Fact, MessageSent, Outbox, OutboxRow, UserId};
use journal::{Journal, JournalError};
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, Notify};

use super::{BoxFuture, ChatRepository};
use crate::common::Message;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Line {
    Sent {
        from: u64,
        to: u64,
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        outbox: Option<Box<OutboxRow>>,
    },
    Published {
        published: u64,
    },
}

type Dialogs = HashMap<(UserId, UserId), Vec<Message>>;

fn pair(a: UserId, b: UserId) -> (UserId, UserId) {
    if a <= b { (a, b) } else { (b, a) }
}

#[derive(Default)]
pub(super) struct State {
    dialogs: Dialogs,
    pub(super) outbox: Outbox,
}

impl State {
    fn store(&mut self, message: Message) {
        self.dialogs
            .entry(pair(message.from, message.to))
            .or_default()
            .push(message);
    }
}

pub struct MemoryChats {
    pub(super) state: Mutex<State>,
    pub(super) journal: Journal<Line>,
    pub(super) signal: Arc<Notify>,
}

impl MemoryChats {
    pub fn new() -> MemoryChats {
        MemoryChats::restore(Journal::memory(), Vec::new())
    }

    pub fn restore(journal: Journal<Line>, lines: Vec<Line>) -> MemoryChats {
        let mut state = State::default();
        for line in lines {
            match line {
                Line::Sent {
                    from,
                    to,
                    text,
                    outbox,
                } => {
                    state.store(Message {
                        from: UserId(from),
                        to: UserId(to),
                        text,
                    });
                    if let Some(row) = outbox {
                        state.outbox.push(*row);
                    }
                }
                Line::Published { published } => state.outbox.published_through(published),
            }
        }
        MemoryChats {
            state: Mutex::new(state),
            journal,
            signal: Arc::default(),
        }
    }
}

impl Default for MemoryChats {
    fn default() -> MemoryChats {
        MemoryChats::new()
    }
}

impl ChatRepository for MemoryChats {
    fn append(
        &self,
        message: Message,
        cause: &Cause,
    ) -> BoxFuture<'_, Result<Message, JournalError>> {
        let cause = cause.clone();
        Box::pin(async move {
            let mut state = self.state.lock().await;
            let fact = Fact::MessageSent(MessageSent {
                from_id: message.from,
                to_id: message.to,
                text: message.text.clone(),
            });
            let row = state.outbox.prepare(&fact, &cause);
            let line = Line::Sent {
                from: message.from.0,
                to: message.to.0,
                text: message.text.clone(),
                outbox: Some(Box::new(row.clone())),
            };
            self.journal.append(&line).await?;
            state.store(message.clone());
            state.outbox.push(row);
            self.signal.notify_one();
            Ok(message)
        })
    }

    fn history(&self, user: UserId, peer: UserId, limit: usize) -> BoxFuture<'_, Vec<Message>> {
        Box::pin(async move {
            let state = self.state.lock().await;
            match state.dialogs.get(&pair(user, peer)) {
                Some(dialog) => dialog[dialog.len().saturating_sub(limit)..].to_vec(),
                None => Vec::new(),
            }
        })
    }
}
