use std::io::{self, Write};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{Error, Result};
use crate::errors::ErrorBody;
use crate::ids::UserId;

/// ```
/// use contracts::{Envelope, decode};
/// use serde_json::json;
///
/// let raw = r#"{"id": 1, "service": "users", "method": "users.find", "data": {"username": "alice"}}"#;
/// let envelope: Envelope = decode(raw)?;
/// assert_eq!(envelope.method, "users.find");
/// assert_eq!(envelope.data, json!({"username": "alice"}));
///
/// let made = Envelope::new(2, "posts.feed", json!({"user": "u_1"}));
/// assert_eq!(made.service, "posts");
/// # Ok::<(), contracts::Error>(())
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Envelope<T = Value> {
    pub id: u64,
    pub service: String,
    pub method: String,
    pub data: T,
}

impl<T> Envelope<T> {
    pub fn new(id: u64, method: &str, data: T) -> Envelope<T> {
        Envelope {
            id,
            service: service_of(method).to_string(),
            method: method.to_string(),
            data,
        }
    }
}

impl Envelope {
    pub fn act_as(&mut self, user: UserId) -> Result<()> {
        let method = self.method.as_str();
        let Some(data) = self.data.as_object_mut() else {
            return Err(Error::bad_data(method, "data должен быть объектом"));
        };
        let me = user.to_string();
        match data.get("user") {
            None => {
                data.insert(String::from("user"), Value::String(me));
                Ok(())
            }
            Some(Value::String(claimed)) if *claimed == me => Ok(()),
            Some(claimed) => Err(Error::Forbidden {
                method: method.to_string(),
                claimed: claimed.to_string(),
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome<T> {
    Ok(T),
    Error(ErrorBody),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Response<T> {
    pub id: Option<u64>,
    #[serde(flatten)]
    pub outcome: Outcome<T>,
}

pub(crate) fn service_of(method: &str) -> &str {
    match method.split_once('.') {
        Some((service, _)) => service,
        None => method,
    }
}

pub fn decode<T: DeserializeOwned>(raw: &str) -> Result<T> {
    serde_json::from_str(raw).map_err(|error| Error::from_json(&error))
}

pub fn encode<T: Serialize>(value: &T) -> Result<String> {
    serde_json::to_string(value).map_err(|error| Error::from_json(&error))
}

pub fn write_line<T: Serialize>(mut out: impl Write, value: &T) -> io::Result<()> {
    serde_json::to_writer(&mut out, value)?;
    out.write_all(b"\n")
}
