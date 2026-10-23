use contracts::{Call, Changed, Envelope, Push, Reply, UserView, decode_call};
use proto::social::v1::{FindProfileRequest, PairRequest};

use super::{Answer, Ctx, acting, misrouted};
use crate::controller::protocol::contract;

const USERS: &str = "сервис профилей";

pub(super) async fn find(ctx: Ctx, envelope: Envelope) -> Answer {
    let call = decode_call(&envelope).map_err(contract)?;
    let Call::FindUser { username } = call else {
        return Err(misrouted(call.method()));
    };
    let ask = FindProfileRequest {
        username: username.as_ref().to_string(),
    };
    let found = ctx.users().find_profile(ask).await;
    let profile = found.map_err(ctx.failed(USERS))?.into_inner();
    Ok(Reply::User(UserView {
        id: contracts::UserId(profile.id),
        username: profile.username,
    }))
}

pub(super) async fn follow(ctx: Ctx, me: UserView, envelope: Envelope) -> Answer {
    let call = acting(&me, envelope)?;
    let Call::Follow { user, target } = call else {
        return Err(misrouted(call.method()));
    };
    let ask = PairRequest {
        target_id: target.0,
    };
    let followed = ctx.users().follow(ask).await;
    let changed = followed.map_err(ctx.failed(USERS))?.into_inner().changed;
    if changed {
        ctx.push(
            target,
            &Push::Followed {
                to: target,
                by: user,
            },
        );
    }
    Ok(Reply::Changed(Changed { changed }))
}

pub(super) async fn mute(ctx: Ctx, me: UserView, envelope: Envelope) -> Answer {
    let call = acting(&me, envelope)?;
    let Call::Mute { target, .. } = call else {
        return Err(misrouted(call.method()));
    };
    let ask = PairRequest {
        target_id: target.0,
    };
    let muted = ctx.users().mute(ask).await;
    let changed = muted.map_err(ctx.failed(USERS))?.into_inner().changed;
    Ok(Reply::Changed(Changed { changed }))
}
