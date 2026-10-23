use crate::common::{Incoming, Kind};
use nats_engine::subject::token;
use nats_engine::{Envelope, Subject};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct EventBody {
    pub actor: String,
    pub post_id: Option<u64>,
}

#[derive(Debug, PartialEq)]
pub enum Decoded {
    Notify(Incoming),
    Ignore,
    Reject(String),
}

pub fn kind_of(root: &str, subject: &Subject) -> Option<(Kind, String)> {
    todo!()
}

pub fn decode(root: &str, envelope: &Envelope<EventBody>) -> Decoded {
    todo!()
}
