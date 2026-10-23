use contracts::{
    Changed, Envelope, FeedPush, LiveCall, Reply, UserId, UserView, decode_live, encode,
};
use proto::social::v1::WatchRequest;
use proto::social::v1::feed_event::Kind;

use super::posts::view;
use super::{Answer, Ctx, Watching, misrouted};
use crate::protocol::{contract, internal};

const POSTS: &str = "сервис постов";

fn live(me: &UserView, mut envelope: Envelope) -> Result<LiveCall, contracts::ErrorBody> {
    envelope.act_as(me.id).map_err(contract)?;
    decode_live(&envelope).map_err(contract)
}

fn event(to: UserId, kind: Kind) -> FeedPush {
    match kind {
        Kind::History(post) => FeedPush::FeedPost {
            to,
            post: view(post),
            live: false,
        },
        Kind::CaughtUp(..) => FeedPush::FeedCaughtUp { to },
        Kind::Live(post) => FeedPush::FeedPost {
            to,
            post: view(post),
            live: true,
        },
    }
}

pub(super) async fn watch(ctx: Ctx, me: UserView, envelope: Envelope) -> Answer {
    let call = live(&me, envelope)?;
    let LiveCall::Watch { user, recent } = call else {
        return Err(misrouted(call.method()));
    };
    let (Some(outbox), Some(watching)) = (ctx.outbox().cloned(), ctx.watching()) else {
        return Err(internal("живая лента есть только у сокета"));
    };
    let ask = WatchRequest {
        recent: u32::try_from(recent).unwrap_or(u32::MAX),
    };
    let opened = ctx.endless_posts().watch(ask).await;
    let mut stream = opened.map_err(ctx.failed(POSTS))?.into_inner();
    let task = tokio::spawn(async move {
        while let Ok(Some(next)) = stream.message().await {
            let Some(kind) = next.kind else {
                continue;
            };
            let Ok(text) = encode(&event(user, kind)) else {
                continue;
            };
            if !outbox.send(text) {
                return;
            }
        }
    });
    watching.start(task);
    Ok(Reply::Changed(Changed { changed: true }))
}

pub(super) async fn unwatch(ctx: Ctx, me: UserView, envelope: Envelope) -> Answer {
    let call = live(&me, envelope)?;
    let LiveCall::Unwatch { .. } = call else {
        return Err(misrouted(call.method()));
    };
    let changed = ctx.watching().is_some_and(Watching::stop);
    Ok(Reply::Changed(Changed { changed }))
}
