use std::collections::HashMap;

use crate::domain::{DomainError, UserId};

pub const MAX_MESSAGE_LEN: usize = 500;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    sender: UserId,
    recipient: UserId,
    text: String,
}

impl Message {
    pub fn sender(&self) -> UserId {
        self.sender
    }

    pub fn recipient(&self) -> UserId {
        self.recipient
    }

    pub fn text(&self) -> &str {
        &self.text
    }
}

#[derive(Debug, Default)]
pub struct Chats {
    dialogs: HashMap<(UserId, UserId), Vec<Message>>,
}

impl Chats {
    pub fn check(sender: UserId, recipient: UserId, text: &str) -> Result<String, DomainError> {
        if sender == recipient {
            return Err(DomainError::SelfMessage);
        }
        validate_message(text)
    }

    pub fn send(
        &mut self,
        sender: UserId,
        recipient: UserId,
        text: &str,
    ) -> Result<&Message, DomainError> {
        let text = Chats::check(sender, recipient, text)?;
        let dialog = self.dialogs.entry(pair(sender, recipient)).or_default();
        dialog.push(Message {
            sender,
            recipient,
            text,
        });
        Ok(&dialog[dialog.len() - 1])
    }

    pub fn history(&self, user: UserId, peer: UserId, limit: usize) -> &[Message] {
        match self.dialogs.get(&pair(user, peer)) {
            Some(dialog) => &dialog[dialog.len().saturating_sub(limit)..],
            None => &[],
        }
    }
}

fn pair(a: UserId, b: UserId) -> (UserId, UserId) {
    if a.0 <= b.0 { (a, b) } else { (b, a) }
}

fn validate_message(text: &str) -> Result<String, DomainError> {
    let text = text.trim();
    if text.is_empty() {
        return Err(DomainError::EmptyMessage);
    }
    if text.chars().count() > MAX_MESSAGE_LEN {
        return Err(DomainError::MessageTooLong);
    }
    Ok(text.to_string())
}
