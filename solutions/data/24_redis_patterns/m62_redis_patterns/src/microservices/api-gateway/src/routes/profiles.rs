use std::collections::HashMap;

use contracts::{PostView, UserId, UserView};
use proto::social::v1::ProfilesRequest;
use service_runtime::{Deadline, retry};
use tokio::time::timeout;
use tonic::{Code, Status};

use super::Ctx;
use std::time::Duration;

use crate::common::{Claim, Flight, Known, RedisProfiles};
use crate::dto::user_view;

fn transient(status: &Status) -> bool {
    matches!(status.code(), Code::Unavailable | Code::DeadlineExceeded)
}

async fn fetch(ctx: &Ctx, ids: &[UserId]) -> Option<HashMap<UserId, UserView>> {
    let guards = ctx.guards();
    let _seat = guards.user_seats.try_enter().ok()?;
    let pass = guards.user_service.allow().ok()?;
    let deadline = Deadline::within(ctx.part_budget());
    let wanted: Vec<u64> = ids.iter().map(|id| id.0).collect();
    let got = retry(
        &guards.policy,
        &guards.retries,
        Some(deadline),
        transient,
        |_| {
            let mut users = ctx.users();
            let ask = ProfilesRequest {
                ids: wanted.clone(),
            };
            async move {
                match timeout(deadline.left(), users.get_profiles(ask)).await {
                    Ok(answer) => answer,
                    Err(..) => Err(Status::deadline_exceeded("бюджет части ответа вышел")),
                }
            }
        },
    )
    .await;
    match got {
        Ok(list) => {
            pass.success();
            let views = list
                .into_inner()
                .profiles
                .into_iter()
                .map(|profile| user_view(profile, ctx.links()))
                .map(|view| (view.id, view))
                .collect();
            Some(views)
        }
        Err(..) => {
            pass.failure();
            None
        }
    }
}

async fn shared_authors(
    ctx: &Ctx,
    cached: &RedisProfiles,
    ids: Vec<UserId>,
) -> (Vec<UserView>, bool) {
    let mut found = cached.cached(&ids).await;
    let missing: Vec<UserId> = ids
        .iter()
        .copied()
        .filter(|id| !found.contains_key(id))
        .collect();
    let claimed = cached.claim(&missing).await;
    let mut stale = false;
    if !claimed.is_empty() {
        match fetch(ctx, &claimed).await {
            Some(fresh) => {
                let views: Vec<UserView> = fresh.values().cloned().collect();
                cached.store(&views, &claimed).await;
                found.extend(fresh);
            }
            None => stale = true,
        }
    }
    let waiting: Vec<UserId> = missing
        .iter()
        .copied()
        .filter(|id| !claimed.contains(id))
        .collect();
    if !waiting.is_empty() {
        tokio::time::sleep(Duration::from_millis(100)).await;
        found.extend(cached.cached(&waiting).await);
        let still: Vec<UserId> = waiting
            .into_iter()
            .filter(|id| !found.contains_key(id))
            .collect();
        if !still.is_empty() {
            match fetch(ctx, &still).await {
                Some(fresh) => found.extend(fresh),
                None => stale = true,
            }
        }
    }
    let mut views: Vec<UserView> = found.into_values().collect();
    views.sort_by_key(|view| view.id);
    (views, stale)
}

pub(super) async fn authors_of(ctx: &Ctx, posts: &[PostView]) -> (Vec<UserView>, bool) {
    let mut ids: Vec<UserId> = posts.iter().map(|post| post.author).collect();
    ids.sort_unstable();
    ids.dedup();
    if let Some(cached) = &ctx.guards().cached {
        return shared_authors(ctx, cached, ids).await;
    }
    let profiles = &ctx.guards().profiles;
    let mut views = Vec::with_capacity(ids.len());
    let mut mine: Vec<Flight> = Vec::new();
    let mut waits = Vec::new();
    for id in ids {
        match profiles.fresh(id) {
            Some(Known::Found(view)) => views.push(view),
            Some(Known::Missing) => {}
            None => match profiles.claim(id) {
                Claim::Mine(flight) => mine.push(flight),
                Claim::Wait(waiting) => waits.push((id, waiting)),
            },
        }
    }
    let mut stale = false;
    if !mine.is_empty() {
        let wanted: Vec<UserId> = mine.iter().map(|flight| flight.id).collect();
        let fetched = fetch(ctx, &wanted).await;
        for flight in mine {
            let id = flight.id;
            match &fetched {
                Some(found) => match found.get(&id) {
                    Some(view) => {
                        views.push(view.clone());
                        profiles.finish(flight, Some(Known::Found(view.clone())));
                    }
                    None => profiles.finish(flight, Some(Known::Missing)),
                },
                None => {
                    stale = true;
                    profiles.finish(flight, None);
                    views.extend(profiles.stale(id));
                }
            }
        }
    }
    for (id, mut waiting) in waits {
        let _ = timeout(ctx.part_budget(), waiting.wait_for(|done| *done)).await;
        match profiles.fresh(id) {
            Some(Known::Found(view)) => views.push(view),
            Some(Known::Missing) => {}
            None => {
                stale = true;
                views.extend(profiles.stale(id));
            }
        }
    }
    views.sort_by_key(|view| view.id);
    (views, stale)
}
