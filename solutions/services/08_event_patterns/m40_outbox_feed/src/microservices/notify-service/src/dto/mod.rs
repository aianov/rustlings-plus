use contracts::{DEFAULT_NOTICES, DomainEvent, Fact, MAX_NOTICES, NoticeAsk, NoticesView, UserId};
use nats_engine::Envelope;

use crate::common::{Incoming, Notice, Trigger};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decoded {
    Notify(Incoming),
    Ignore,
    Reject(String),
}

fn notify(
    envelope: &Envelope<DomainEvent>,
    kind: Trigger,
    owner: UserId,
    actor: UserId,
    post: Option<contracts::PostId>,
) -> Decoded {
    if kind != Trigger::Posted && actor == owner {
        return Decoded::Ignore;
    }
    Decoded::Notify(Incoming {
        kind,
        owner,
        actor,
        post,
        request_id: envelope.meta.as_ref().map(|meta| meta.request_id.clone()),
    })
}

pub fn decode(envelope: &Envelope<DomainEvent>) -> Decoded {
    let fact = match envelope.body.fact() {
        Ok(Some(fact)) => fact,
        Ok(None) => return Decoded::Ignore,
        Err(error) => return Decoded::Reject(error.to_string()),
    };
    match fact {
        Fact::Followed(fact) => notify(
            envelope,
            Trigger::Followed,
            fact.followee_id,
            fact.follower_id,
            None,
        ),
        Fact::PostLiked(fact) => notify(
            envelope,
            Trigger::Liked,
            fact.author_id,
            fact.user_id,
            Some(fact.post_id),
        ),
        Fact::PostCommented(fact) => notify(
            envelope,
            Trigger::Commented,
            fact.author_id,
            fact.commenter_id,
            Some(fact.post_id),
        ),
        Fact::PostCreated(fact) => notify(
            envelope,
            Trigger::Posted,
            fact.author_id,
            fact.author_id,
            Some(fact.post_id),
        ),
        _ => Decoded::Ignore,
    }
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
