use std::collections::HashMap;

use contracts::{DailyView, DayView, ErrorBody, PeopleView, PostView};
use proto::social::v1::{DailyRequest, PostsByIds, ProfileList, RelatedRequest};
use tokio::time::timeout;
use tonic::{Response, Status};

use super::Ctx;
use crate::common::Principal;
use crate::dto::{DailyAsk, RelatedAsk, user_view};

const USERS: &str = "сервис профилей";

const POSTS: &str = "сервис постов";

fn related(data: &RelatedAsk) -> RelatedRequest {
    RelatedRequest {
        user_id: data.target.0,
        after: data.after.map_or(0, |after| after.0),
        limit: data.limit,
    }
}

fn people(
    ctx: &Ctx,
    limit: u32,
    found: Result<Response<ProfileList>, Status>,
) -> Result<PeopleView, ErrorBody> {
    let list = found.map_err(ctx.failed(USERS))?.into_inner();
    let users: Vec<_> = list
        .profiles
        .into_iter()
        .map(|profile| user_view(profile, ctx.links()))
        .collect();
    let full = u32::try_from(users.len()).is_ok_and(|len| len == limit && limit > 0);
    let next = users.last().filter(|_| full).map(|user| user.id);
    Ok(PeopleView { users, next })
}

pub(super) async fn followers(ctx: Ctx, data: RelatedAsk) -> Result<PeopleView, ErrorBody> {
    let found = ctx.users().followers(related(&data)).await;
    people(&ctx, data.limit, found)
}

pub(super) async fn following(ctx: Ctx, data: RelatedAsk) -> Result<PeopleView, ErrorBody> {
    let found = ctx.users().following(related(&data)).await;
    people(&ctx, data.limit, found)
}

pub(super) async fn friends(ctx: Ctx, data: RelatedAsk) -> Result<PeopleView, ErrorBody> {
    let found = ctx.users().friends(related(&data)).await;
    people(&ctx, data.limit, found)
}

pub(super) async fn daily(ctx: Ctx, _: Principal, data: DailyAsk) -> Result<DailyView, ErrorBody> {
    let ask = DailyRequest { days: data.days };
    let reply = ctx.posts().daily_stats(ask).await;
    let reply = reply.map_err(ctx.failed(POSTS))?.into_inner();
    let count = |value: u64| usize::try_from(value).unwrap_or(usize::MAX);
    Ok(DailyView {
        days: reply
            .days
            .into_iter()
            .map(|day| DayView {
                day: day.day,
                posts: count(day.posts),
                likes: count(day.likes),
            })
            .collect(),
    })
}

pub(super) async fn hydrated(ctx: &Ctx, posts: Vec<PostView>) -> Vec<PostView> {
    if posts.is_empty() {
        return posts;
    }
    let ask = PostsByIds {
        ids: posts.iter().map(|post| post.id.0).collect(),
        hidden_authors: Vec::new(),
    };
    let mut client = ctx.posts();
    let fresh = match timeout(ctx.part_budget(), client.get_posts(ask)).await {
        Ok(Ok(reply)) => reply.into_inner().posts,
        _ => {
            tracing::warn!(
                "api-gateway [{}]: гидрация ленты не пришла, лайки без отметок",
                ctx.request_id()
            );
            return posts;
        }
    };
    let liked: HashMap<u64, bool> = fresh.iter().map(|post| (post.id, post.liked)).collect();
    posts
        .into_iter()
        .filter_map(|mut post| {
            let mine = liked.get(&post.id.0)?;
            post.liked = *mine;
            Some(post)
        })
        .collect()
}
