use serde_json::{Map, Value};

use super::{DomainEvent, FactError, PROFILE_SCHEMA};

fn bad(event: &DomainEvent, reason: &str) -> FactError {
    FactError::BadPayload {
        kind: event.kind.clone(),
        reason: reason.to_string(),
    }
}

fn profile_v1_to_v2(mut event: DomainEvent) -> Result<DomainEvent, FactError> {
    let name = match event.payload.get("name") {
        Some(Value::String(name)) => name.trim().to_string(),
        Some(Value::Null) => String::new(),
        Some(_) => return Err(bad(&event, "name не строка")),
        None => return Err(bad(&event, "в схеме 1 нет поля name")),
    };
    let mut payload = match event.payload {
        Value::Object(fields) => fields,
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) | Value::Array(_) => {
            Map::new()
        }
    };
    payload.remove("name");
    payload
        .entry("user_id")
        .or_insert_with(|| Value::String(event.aggregate_id.clone()));
    let display_name = match name.is_empty() {
        true => Value::Null,
        false => Value::String(name),
    };
    payload.insert(String::from("display_name"), display_name);
    event.payload = Value::Object(payload);
    event.schema = PROFILE_SCHEMA;
    Ok(event)
}

fn newest(kind: &str) -> u32 {
    if kind == "user.profile_changed" {
        PROFILE_SCHEMA
    } else {
        1
    }
}

pub fn upcast(event: DomainEvent) -> Result<DomainEvent, FactError> {
    let lifting = event.kind == "user.profile_changed" && event.schema == 1;
    let event = if lifting {
        profile_v1_to_v2(event)?
    } else {
        event
    };
    let newest = newest(&event.kind);
    if event.schema == newest {
        Ok(event)
    } else {
        Err(FactError::UnknownSchema {
            kind: event.kind,
            schema: event.schema,
        })
    }
}
