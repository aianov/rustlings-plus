use serde::{Deserialize, Serialize};

pub const MAX_URL_LEN: usize = 2048;
pub const MAX_EVENTS: usize = 20;
pub const MAX_IDEMPOTENCY_KEY_LEN: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SubscribeRequest {
    pub url: String,
    pub events: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FieldError {
    pub path: String,
    pub code: &'static str,
}

impl FieldError {
    pub fn new(path: impl Into<String>, code: &'static str) -> FieldError {
        FieldError {
            path: path.into(),
            code,
        }
    }
}

impl SubscribeRequest {
    pub fn validate(&self) -> Result<(), Vec<FieldError>> {
        let mut errors = Vec::new();
        match self.url.strip_prefix("https://") {
            None => errors.push(FieldError::new("url", "scheme")),
            Some(rest) => {
                if rest.split(['/', '?', '#']).next().unwrap_or("").is_empty() {
                    errors.push(FieldError::new("url", "host"));
                }
            }
        }
        if self.url.len() > MAX_URL_LEN {
            errors.push(FieldError::new("url", "too_long"));
        }
        if self.events.is_empty() {
            errors.push(FieldError::new("events", "empty"));
        } else if self.events.len() > MAX_EVENTS {
            errors.push(FieldError::new("events", "too_many"));
        }
        for (i, event) in self.events.iter().enumerate() {
            if event != "*" && !is_event_type(event) {
                errors.push(FieldError::new(format!("events[{i}]"), "format"));
            } else if self.events[..i].contains(event) {
                errors.push(FieldError::new(format!("events[{i}]"), "duplicate"));
            }
        }
        if errors.is_empty() { Ok(()) } else { Err(errors) }
    }
}

pub fn is_event_type(s: &str) -> bool {
    s.split_once('.')
        .is_some_and(|(group, action)| is_word(group) && is_word(action))
}

fn is_word(s: &str) -> bool {
    s.starts_with(|c: char| c.is_ascii_lowercase())
        && s.chars().all(|c| c.is_ascii_lowercase() || c == '_')
}

pub fn is_idempotency_key(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= MAX_IDEMPOTENCY_KEY_LEN
        && s.bytes().all(|b| b.is_ascii_graphic())
}
