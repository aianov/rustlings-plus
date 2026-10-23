use std::sync::{Arc, Mutex, PoisonError};

use service_runtime::Trail;

use crate::service::{Letter, Mailer};

pub struct LogMailer {
    trail: Trail,
}

impl LogMailer {
    pub fn new(trail: Trail) -> LogMailer {
        LogMailer { trail }
    }
}

impl Mailer for LogMailer {
    fn send(&self, letter: Letter) {
        self.trail.note(&format!(
            "письмо для {}: «{}» {}",
            letter.to, letter.subject, letter.link
        ));
    }
}

#[derive(Debug, Clone, Default)]
pub struct Mailbox {
    letters: Arc<Mutex<Vec<Letter>>>,
}

impl Mailbox {
    pub fn letters(&self) -> Vec<Letter> {
        self.letters
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl Mailer for Mailbox {
    fn send(&self, letter: Letter) {
        self.letters
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(letter);
    }
}
