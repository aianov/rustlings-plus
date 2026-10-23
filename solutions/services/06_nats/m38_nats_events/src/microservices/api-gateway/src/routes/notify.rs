use contracts::{
    DEFAULT_NOTICES, ErrorBody, ErrorCode, MAX_NOTICES, MarkedView, NoticeAsk, NoticesView,
    NotifyMethod, UnreadView,
};

use super::Ctx;
use crate::common::Principal;
use crate::dto::{Nothing, NoticesAsk};

pub(super) async fn unread(ctx: Ctx, me: Principal, _: Nothing) -> Result<UnreadView, ErrorBody> {
    let ask = NoticeAsk {
        user: me.id,
        limit: None,
    };
    ctx.notify(NotifyMethod::Unread, &ask).await
}

pub(super) async fn list(
    ctx: Ctx,
    me: Principal,
    data: NoticesAsk,
) -> Result<NoticesView, ErrorBody> {
    let limit = data.limit.unwrap_or(DEFAULT_NOTICES);
    if !(1..=MAX_NOTICES).contains(&limit) {
        let reason = format!("limit от 1 до {MAX_NOTICES}, а не {limit}");
        return Err(ErrorBody::new(ErrorCode::BadData, reason));
    }
    let ask = NoticeAsk {
        user: me.id,
        limit: Some(limit),
    };
    ctx.notify(NotifyMethod::List, &ask).await
}

pub(super) async fn read(ctx: Ctx, me: Principal, _: Nothing) -> Result<MarkedView, ErrorBody> {
    let ask = NoticeAsk {
        user: me.id,
        limit: None,
    };
    ctx.notify(NotifyMethod::Read, &ask).await
}
