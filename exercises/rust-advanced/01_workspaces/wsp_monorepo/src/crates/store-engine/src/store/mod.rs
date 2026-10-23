use contracts::ids::UserId;
use contracts::message::Message;

use crate::index::AuthorIndex;

#[derive(Debug, Default)]
pub struct Store {
    messages: Vec<Message>,
    index: AuthorIndex,
}

impl Store {
    pub fn new() -> Store {
        Store::default()
    }

    pub fn push(&mut self, message: Message) {
        self.index.record(message.from, self.messages.len());
        self.messages.push(message);
    }

    pub fn len(&self) -> usize {
        self.messages.len()
    }

    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    pub fn authors(&self) -> usize {
        self.index.authors()
    }

    pub fn by_author(&self, author: UserId) -> Vec<&Message> {
        self.index.positions(author).iter().map(|i| &self.messages[*i]).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indexes_by_author() {
        let mut store = Store::new();
        store.push(Message::new(UserId(1), "a"));
        store.push(Message::new(UserId(2), "b"));
        store.push(Message::new(UserId(1), "c"));
        assert_eq!(store.by_author(UserId(1)).len(), 2);
        assert_eq!(store.authors(), 2);
    }
}
