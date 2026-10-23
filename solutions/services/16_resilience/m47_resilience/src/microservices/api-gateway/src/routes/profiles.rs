use contracts::{PostView, UserId, UserView};
use proto::social::v1::GetProfileRequest;
use service_runtime::{Deadline, retry};
use tokio::time::timeout;
use tonic::{Code, Status};

use super::Ctx;
use crate::dto::user_view;

enum Fetched {
    Fresh(UserView),
    Gone,
    Failed,
}

fn transient(status: &Status) -> bool {
    matches!(status.code(), Code::Unavailable | Code::DeadlineExceeded)
}

async fn fetch(ctx: &Ctx, id: UserId) -> Fetched {
    let guards = ctx.guards();
    let Ok(_seat) = guards.user_seats.try_enter() else {
        return Fetched::Failed;
    };
    let Ok(pass) = guards.user_service.allow() else {
        return Fetched::Failed;
    };
    let deadline = Deadline::within(ctx.part_budget());
    let got = retry(
        &guards.policy,
        &guards.retries,
        Some(deadline),
        transient,
        |_| {
            let mut users = ctx.users();
            async move {
                let ask = GetProfileRequest { user_id: id.0 };
                match timeout(deadline.left(), users.get_profile(ask)).await {
                    Ok(answer) => answer,
                    Err(..) => Err(Status::deadline_exceeded("бюджет части ответа вышел")),
                }
            }
        },
    )
    .await;
    match got {
        Ok(profile) => {
            pass.success();
            Fetched::Fresh(user_view(profile.into_inner(), ctx.links()))
        }
        Err(status) if status.code() == Code::NotFound => {
            pass.success();
            Fetched::Gone
        }
        Err(..) => {
            pass.failure();
            Fetched::Failed
        }
    }
}

pub(super) async fn authors_of(ctx: &Ctx, posts: &[PostView]) -> (Vec<UserView>, bool) {
    let mut ids: Vec<UserId> = posts.iter().map(|post| post.author).collect();
    ids.sort_unstable();
    ids.dedup();
    let profiles = &ctx.guards().profiles;
    let mut views = Vec::with_capacity(ids.len());
    let mut stale = false;
    for id in ids {
        if let Some(view) = profiles.fresh(id) {
            views.push(view);
            continue;
        }
        match fetch(ctx, id).await {
            Fetched::Fresh(view) => {
                profiles.put(view.clone());
                views.push(view);
            }
            Fetched::Gone => {}
            Fetched::Failed => {
                stale = true;
                if let Some(old) = profiles.stale(id) {
                    views.push(old);
                }
            }
        }
    }
    (views, stale)
}
