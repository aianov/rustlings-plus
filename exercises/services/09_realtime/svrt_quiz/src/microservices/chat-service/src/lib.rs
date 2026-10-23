use bus::{Bus, typing_subject, user_subject, valid_word};

#[derive(Debug, PartialEq)]
pub enum ChatError {
    BadUser,
    BadText,
}

pub struct ChatService {
    bus: Bus,
}

impl ChatService {
    pub fn new(bus: Bus) -> ChatService {
        ChatService { bus }
    }

    pub fn send(&self, from: &str, to: &str, text: &str) -> Result<u64, ChatError> {
        todo!()
    }

    pub fn typing(&self, from: &str, to: &str) -> Result<(), ChatError> {
        todo!()
    }
}
