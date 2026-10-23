use crate::Event;
use serde_json::{Value, json};

#[derive(Debug, PartialEq)]
pub enum UpcastError {
    UnknownSchema(u32),
    BadPayload(&'static str),
}

pub fn upcast(event: Event) -> Result<Event, UpcastError> {
    todo!()
}
