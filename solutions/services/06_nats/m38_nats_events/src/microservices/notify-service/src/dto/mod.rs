use contracts::{
    DEFAULT_NOTICES, EventBody, EventKind, MAX_NOTICES, NoticeAsk, NoticesView, Topics,
};
use nats_engine::Envelope;

use crate::common::{Incoming, Notice};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decoded {
    Notify(Incoming),
    Ignore,
    Reject(String),
}

pub fn decode(topics: &Topics, envelope: &Envelope<EventBody>) -> Decoded {
    let Some((kind, owner)) = topics.read_event(envelope.subject.as_str()) else {
        return Decoded::Ignore;
    };
    let body = &envelope.body;
    let post = match (kind, body.post) {
        (EventKind::UserFollowed, _) => None,
        (_, Some(post)) => Some(post),
        (_, None) => return Decoded::Reject(format!("у события {kind:?} нет поля post")),
    };
    match kind {
        EventKind::PostCreated if body.actor != owner => {
            return Decoded::Reject(format!(
                "пост {} создал {}, а тема называет {owner}",
                body.post.map(|post| post.to_string()).unwrap_or_default(),
                body.actor
            ));
        }
        EventKind::PostCreated => {}
        _ if body.actor == owner => return Decoded::Ignore,
        _ => {}
    }
    Decoded::Notify(Incoming {
        kind,
        owner,
        actor: body.actor,
        post,
        request_id: envelope.meta.as_ref().map(|meta| meta.request_id.clone()),
    })
}

pub fn limit(ask: &NoticeAsk) -> Result<usize, String> {
    let limit = ask.limit.unwrap_or(DEFAULT_NOTICES);
    if (1..=MAX_NOTICES).contains(&limit) {
        Ok(limit as usize)
    } else {
        Err(format!("limit {limit} вне 1..={MAX_NOTICES}"))
    }
}

pub fn notices(list: &[Notice], unread: u32) -> NoticesView {
    NoticesView {
        notices: list.iter().map(Notice::view).collect(),
        unread,
    }
}
