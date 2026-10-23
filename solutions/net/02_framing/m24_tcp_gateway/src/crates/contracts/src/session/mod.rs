use serde::Deserialize;

use crate::envelope::{Envelope, service_of};
use crate::error::{Error, Result};
use crate::username::Username;

pub const SESSION_METHODS: [&str; 2] = ["session.hello", "session.bye"];

pub const ANONYMOUS_METHODS: [&str; 2] = ["users.register", "users.find"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionCall {
    Hello { username: Username },
    Bye,
}

#[derive(Deserialize)]
struct HelloData {
    username: Username,
}

/// ```
/// use contracts::{Envelope, SessionCall, decode, decode_session};
///
/// let raw = r#"{"id": 1, "service": "session", "method": "session.hello", "data": {"username": "Alice"}}"#;
/// let envelope: Envelope = decode(raw)?;
/// let call = decode_session(&envelope)?;
/// assert!(matches!(call, SessionCall::Hello { username } if username.as_ref() == "alice"));
/// # Ok::<(), contracts::Error>(())
/// ```
pub fn decode_session(envelope: &Envelope) -> Result<SessionCall> {
    let method = envelope.method.as_str();
    if !SESSION_METHODS.contains(&method) {
        return Err(Error::UnknownMethod(method.to_string()));
    }
    if service_of(method) != envelope.service {
        return Err(Error::WrongService {
            service: envelope.service.to_string(),
            method: method.to_string(),
        });
    }
    match method {
        "session.hello" => HelloData::deserialize(&envelope.data)
            .map(|data| SessionCall::Hello {
                username: data.username,
            })
            .map_err(|error| Error::bad_data(method, error)),
        "session.bye" => Ok(SessionCall::Bye),
        other => Err(Error::UnknownMethod(other.to_string())),
    }
}

pub fn needs_actor(method: &str) -> bool {
    !ANONYMOUS_METHODS.contains(&method) && !SESSION_METHODS.contains(&method)
}
