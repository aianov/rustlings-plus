use serde::Deserialize;

use crate::envelope::{Envelope, service_of};
use crate::error::{Error, Result};
use crate::ids::UserId;

pub const LIVE_METHODS: [&str; 2] = ["posts.watch", "posts.unwatch"];

pub const DEFAULT_RECENT: usize = 20;

pub const MAX_RECENT: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveCall {
    Watch { user: UserId, recent: usize },
    Unwatch { user: UserId },
}

impl LiveCall {
    pub fn method(&self) -> &'static str {
        match self {
            LiveCall::Watch { .. } => "posts.watch",
            LiveCall::Unwatch { .. } => "posts.unwatch",
        }
    }
}

#[derive(Deserialize)]
struct WatchData {
    user: UserId,
    #[serde(default = "default_recent")]
    recent: usize,
}

#[derive(Deserialize)]
struct UnwatchData {
    user: UserId,
}

fn default_recent() -> usize {
    DEFAULT_RECENT
}

pub fn decode_live(envelope: &Envelope) -> Result<LiveCall> {
    let method = envelope.method.as_str();
    if !LIVE_METHODS.contains(&method) {
        return Err(Error::UnknownMethod(method.to_string()));
    }
    if service_of(method) != envelope.service {
        return Err(Error::WrongService {
            service: envelope.service.to_string(),
            method: method.to_string(),
        });
    }
    if method == "posts.unwatch" {
        let data = UnwatchData::deserialize(&envelope.data);
        let data = data.map_err(|error| Error::bad_data(method, error))?;
        return Ok(LiveCall::Unwatch { user: data.user });
    }
    let data = WatchData::deserialize(&envelope.data);
    let data = data.map_err(|error| Error::bad_data(method, error))?;
    if !(1..=MAX_RECENT).contains(&data.recent) {
        let reason = format!("recent от 1 до {MAX_RECENT}, а не {}", data.recent);
        return Err(Error::bad_data(method, reason));
    }
    Ok(LiveCall::Watch {
        user: data.user,
        recent: data.recent,
    })
}
