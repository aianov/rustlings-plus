use std::future::Future;
use std::time::Duration;

use contracts::{Changed, ErrorBody, ProfileView, StatsView, UserView};
use proto::social::v1::{
    AuthorPostsRequest, FindProfileRequest, GetProfileRequest, PairRequest, RelationsRequest,
};
use tokio::time::timeout;
use tonic::Status;

use super::Ctx;
use crate::common::Principal;
use crate::dto::{FindUser, Target, post_view, user_view};

const USERS: &str = "сервис профилей";

pub const PROFILE_POSTS: u32 = 10;

pub(super) async fn find(ctx: Ctx, data: FindUser) -> Result<UserView, ErrorBody> {
    let ask = FindProfileRequest {
        username: data.username.as_ref().to_string(),
    };
    let found = ctx.users().find_profile(ask).await;
    Ok(user_view(found.map_err(ctx.failed(USERS))?.into_inner()))
}

pub(super) async fn follow(ctx: Ctx, _: Principal, data: Target) -> Result<Changed, ErrorBody> {
    let ask = PairRequest {
        target_id: data.target.0,
    };
    let followed = ctx.users().follow(ask).await;
    let changed = followed.map_err(ctx.failed(USERS))?.into_inner().changed;
    Ok(Changed { changed })
}

pub(super) async fn mute(ctx: Ctx, _: Principal, data: Target) -> Result<Changed, ErrorBody> {
    let ask = PairRequest {
        target_id: data.target.0,
    };
    let muted = ctx.users().mute(ask).await;
    let changed = muted.map_err(ctx.failed(USERS))?.into_inner().changed;
    Ok(Changed { changed })
}

async fn part<T>(
    ctx: &Ctx,
    name: &str,
    budget: Duration,
    work: impl Future<Output = Result<tonic::Response<T>, Status>>,
) -> Option<T> {
    match timeout(budget, work).await {
        Ok(Ok(reply)) => Some(reply.into_inner()),
        Ok(Err(status)) => {
            eprintln!(
                "api-gateway [{}]: часть профиля {name} не пришла: {:?} {}",
                ctx.request_id(),
                status.code(),
                status.message()
            );
            None
        }
        Err(..) => {
            eprintln!(
                "api-gateway [{}]: часть профиля {name} не уложилась в {budget:?}",
                ctx.request_id()
            );
            None
        }
    }
}

pub(super) async fn profile(ctx: Ctx, data: Target) -> Result<ProfileView, ErrorBody> {
    let id = data.target.0;
    let budget = ctx.part_budget();
    let (mut users, mut stats_client, mut posts) = (ctx.users(), ctx.users(), ctx.posts());
    let user = users.get_profile(GetProfileRequest { user_id: id });
    let recent = AuthorPostsRequest {
        author_id: id,
        limit: PROFILE_POSTS,
    };
    let (user, posts, relations) = tokio::join!(
        user,
        part(&ctx, "posts", budget, posts.author_posts(recent)),
        part(
            &ctx,
            "stats",
            budget,
            stats_client.relations(RelationsRequest { user_id: id })
        ),
    );
    let user = user_view(user.map_err(ctx.failed(USERS))?.into_inner());
    let posts = posts.map(|reply| reply.posts.into_iter().map(post_view).collect());
    let stats = relations.map(|reply| StatsView {
        followers: reply.followers.len(),
        following: reply.following.len(),
    });
    let mut partial = Vec::new();
    if posts.is_none() {
        partial.push(String::from("posts"));
    }
    if stats.is_none() {
        partial.push(String::from("stats"));
    }
    Ok(ProfileView {
        user,
        posts,
        stats,
        partial,
    })
}
