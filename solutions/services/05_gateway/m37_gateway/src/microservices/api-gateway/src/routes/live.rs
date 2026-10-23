use contracts::{Changed, ErrorBody, ErrorCode, MAX_RECENT, encode};
use proto::social::v1::WatchRequest;

use super::{Ctx, Watching};
use crate::common::Principal;
use crate::controller::protocol::internal;
use crate::dto::{Nothing, WatchAsk, feed_event as event};

const POSTS: &str = "сервис постов";

pub(super) async fn watch(ctx: Ctx, me: Principal, data: WatchAsk) -> Result<Changed, ErrorBody> {
    if !(1..=MAX_RECENT).contains(&data.recent) {
        let reason = format!("recent от 1 до {MAX_RECENT}, а не {}", data.recent);
        return Err(ErrorBody::new(ErrorCode::BadData, reason));
    }
    let (Some(outbox), Some(watching)) = (ctx.outbox().cloned(), ctx.watching()) else {
        return Err(internal("живая лента есть только у сокета"));
    };
    let ask = WatchRequest {
        recent: u32::try_from(data.recent).unwrap_or(u32::MAX),
    };
    let opened = ctx.endless_posts().watch(ask).await;
    let mut stream = opened.map_err(ctx.failed(POSTS))?.into_inner();
    let user = me.id;
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
    Ok(Changed { changed: true })
}

pub(super) async fn unwatch(ctx: Ctx, _: Principal, _: Nothing) -> Result<Changed, ErrorBody> {
    let changed = ctx.watching().is_some_and(Watching::stop);
    Ok(Changed { changed })
}
