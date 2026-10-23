use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex};

pub type Headers = BTreeMap<String, String>;

#[derive(Debug, Clone)]
pub struct Message {
    pub subject: String,
    pub headers: Headers,
    pub payload: String,
}

#[derive(Clone, Default)]
pub struct Bus {
    queue: Arc<Mutex<VecDeque<Message>>>,
}

impl Bus {
    pub fn publish(&self, message: Message) {
        self.queue.lock().expect("очередь не отравлена").push_back(message);
    }

    pub fn next(&self) -> Option<Message> {
        self.queue.lock().expect("очередь не отравлена").pop_front()
    }

    pub fn len(&self) -> usize {
        self.queue.lock().expect("очередь не отравлена").len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
