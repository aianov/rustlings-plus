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
        if !valid_word(from) || !valid_word(to) {
            return Err(ChatError::BadUser);
        }
        let text = text.trim();
        if text.is_empty() || text.chars().count() > 500 {
            return Err(ChatError::BadText);
        }
        Ok(self.bus.append(&user_subject(to), &format!("{from}: {text}")))
    }

    pub fn typing(&self, from: &str, to: &str) -> Result<(), ChatError> {
        if !valid_word(from) || !valid_word(to) {
            return Err(ChatError::BadUser);
        }
        self.bus.publish(&typing_subject(to), from);
        Ok(())
    }
}
