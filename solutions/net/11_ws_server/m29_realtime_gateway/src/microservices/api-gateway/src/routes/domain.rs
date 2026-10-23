use contracts::{Envelope, UserView, decode_call};
use social_core::serve;

use super::{Answer, Ctx};
use crate::protocol::contract;

pub(super) async fn open(ctx: Ctx, envelope: Envelope) -> Answer {
    let call = decode_call(&envelope).map_err(contract)?;
    serve(ctx.app(), &call)
        .await
        .map_err(|error| error.to_body())
}

pub(super) async fn acting(ctx: Ctx, me: UserView, mut envelope: Envelope) -> Answer {
    envelope.act_as(me.id).map_err(contract)?;
    open(ctx, envelope).await
}
