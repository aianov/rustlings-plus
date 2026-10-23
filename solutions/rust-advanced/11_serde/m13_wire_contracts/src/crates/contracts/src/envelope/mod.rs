use std::io::{self, Write};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{Error, ErrorBody, Result};

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

pub fn write_line<T: Serialize>(mut out: impl Write, value: &T) -> io::Result<()> {
    serde_json::to_writer(&mut out, value)?;
    out.write_all(b"\n")
}
