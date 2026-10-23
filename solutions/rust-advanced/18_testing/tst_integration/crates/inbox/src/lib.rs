#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    pub id: u32,
    pub from: String,
    pub text: String,
    pub read: bool,
}

#[derive(Debug, PartialEq)]
pub enum InboxError {
    Empty,
    TooLong(usize),
    NotFound(u32),
}

pub const MAX_LEN: usize = 200;

pub struct Inbox {
    messages: Vec<Message>,
    next_id: u32,
}

fn normalize(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

impl Inbox {
    pub fn new() -> Inbox {
        Inbox { messages: Vec::new(), next_id: 1 }
    }

    pub fn receive(&mut self, from: &str, text: &str) -> Result<u32, InboxError> {
        let text = normalize(text);
        if text.is_empty() {
            return Err(InboxError::Empty);
        }
        let length = text.chars().count();
        if length > MAX_LEN {
            return Err(InboxError::TooLong(length));
        }
        let id = self.next_id;
        self.next_id += 1;
        self.messages.push(Message { id, from: from.to_string(), text, read: false });
        Ok(id)
    }

    pub fn mark_read(&mut self, id: u32) -> Result<(), InboxError> {
        let message = self.messages.iter_mut().find(|m| m.id == id).ok_or(InboxError::NotFound(id))?;
        message.read = true;
        Ok(())
    }

    pub fn get(&self, id: u32) -> Option<&Message> {
        self.messages.iter().find(|m| m.id == id)
    }

    pub fn unread(&self) -> Vec<&Message> {
        self.messages.iter().filter(|m| !m.read).collect()
    }

    pub fn len(&self) -> usize {
        self.messages.len()
    }

    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    pub fn clear_read(&mut self) -> usize {
        let before = self.messages.len();
        self.messages.retain(|m| !m.read);
        before - self.messages.len()
    }

    pub fn search(&self, term: &str) -> Vec<u32> {
        let term = term.to_lowercase();
        self.messages.iter().filter(|m| m.text.to_lowercase().contains(&term)).map(|m| m.id).collect()
    }
}

impl Default for Inbox {
    fn default() -> Self {
        Inbox::new()
    }
}
