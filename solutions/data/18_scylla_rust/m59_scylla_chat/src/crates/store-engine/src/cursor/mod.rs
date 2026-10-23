use std::fmt::Write;

use bytes::Bytes;

use crate::error::StoreError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cursor {
    pub bucket: i32,
    pub state: Option<Bytes>,
}

pub fn encode_state(state: &[u8]) -> String {
    let mut out = String::with_capacity(state.len() * 2);
    for byte in state {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

pub fn decode_state(hex: &str) -> Option<Bytes> {
    if hex.len() % 2 != 0 {
        return None;
    }
    (0..hex.len())
        .step_by(2)
        .map(|at| {
            hex.get(at..at + 2)
                .and_then(|pair| u8::from_str_radix(pair, 16).ok())
        })
        .collect::<Option<Vec<u8>>>()
        .map(Bytes::from)
}

impl Cursor {
    pub fn encode(&self) -> String {
        let state = self.state.as_deref().map(encode_state).unwrap_or_default();
        format!("{}.{state}", self.bucket)
    }

    pub fn decode(raw: &str) -> Result<Cursor, StoreError> {
        let (bucket, state) = raw.split_once('.').ok_or(StoreError::BadCursor)?;
        let bucket = bucket.parse().map_err(|_| StoreError::BadCursor)?;
        let state = match state {
            "" => None,
            hex => Some(decode_state(hex).ok_or(StoreError::BadCursor)?),
        };
        Ok(Cursor { bucket, state })
    }
}
