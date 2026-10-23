use crate::Event;
use serde_json::{Value, json};

#[derive(Debug, PartialEq)]
pub enum UpcastError {
    UnknownSchema(u32),
    BadPayload(&'static str),
}

pub fn upcast(event: Event) -> Result<Event, UpcastError> {
    let mut event = event;
    if event.kind != "user.profile_changed" {
        return Ok(event);
    }
    if event.schema == 1 {
        let name = event.payload.get("name").and_then(Value::as_str).ok_or(UpcastError::BadPayload("name"))?.trim().to_string();
        if name.is_empty() {
            return Err(UpcastError::BadPayload("name"));
        }
        let (first, last) = match name.split_once(char::is_whitespace) {
            Some((first, last)) => (first.to_string(), last.trim_start().to_string()),
            None => (name, String::new()),
        };
        let object = event.payload.as_object_mut().ok_or(UpcastError::BadPayload("payload"))?;
        object.remove("name");
        object.insert("first_name".to_string(), json!(first));
        object.insert("last_name".to_string(), json!(last));
        event.schema = 2;
    }
    match event.schema {
        2 => Ok(event),
        other => Err(UpcastError::UnknownSchema(other)),
    }
}
