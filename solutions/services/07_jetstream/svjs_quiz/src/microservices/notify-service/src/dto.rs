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
    let tokens: Vec<&str> = subject.as_str().split('.').collect();
    let [first, entity, event, user] = tokens.as_slice() else { return None };
    if *first != root {
        return None;
    }
    let kind = match (*entity, *event) {
        ("post", "liked") => Kind::Liked,
        ("post", "commented") => Kind::Commented,
        ("user", "followed") => Kind::Followed,
        _ => return None,
    };
    Some((kind, user.to_string()))
}

pub fn decode(root: &str, envelope: &Envelope<EventBody>) -> Decoded {
    let Some((kind, user)) = kind_of(root, &envelope.subject) else { return Decoded::Ignore };
    if token(&envelope.body.actor).is_err() {
        return Decoded::Reject(format!("актор {:?} не годится", envelope.body.actor));
    }
    if kind != Kind::Followed && envelope.body.post_id.is_none() {
        return Decoded::Reject("у события нет post_id".to_string());
    }
    if envelope.body.actor == user {
        return Decoded::Ignore;
    }
    let post_id = if kind == Kind::Followed { None } else { envelope.body.post_id };
    Decoded::Notify(Incoming {
        user,
        kind,
        actor: envelope.body.actor.clone(),
        post_id,
        request_id: envelope.meta.as_ref().map(|m| m.request_id.clone()),
    })
}
