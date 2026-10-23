use contracts::{Changed, CommentLikeView, ErrorBody, FeedPage};
use proto::social::v1::{AuthorPostsRequest, CommentAction, PostAction};

use super::Ctx;
use crate::common::Principal;
use crate::dto::{CommentRef, PostRef, Target, post_view};

const POSTS: &str = "сервис постов";

pub(super) async fn pin(ctx: Ctx, _: Principal, data: PostRef) -> Result<Changed, ErrorBody> {
    let ask = PostAction {
        post_id: data.post.0,
    };
    let pinned = ctx.posts().pin(ask).await;
    let changed = pinned.map_err(ctx.failed(POSTS))?.into_inner().changed;
    Ok(Changed { changed })
}

pub(super) async fn unpin(ctx: Ctx, _: Principal, data: PostRef) -> Result<Changed, ErrorBody> {
    let ask = PostAction {
        post_id: data.post.0,
    };
    let unpinned = ctx.posts().unpin(ask).await;
    let changed = unpinned.map_err(ctx.failed(POSTS))?.into_inner().changed;
    Ok(Changed { changed })
}

pub(super) async fn pinned(ctx: Ctx, data: Target) -> Result<FeedPage, ErrorBody> {
    let ask = AuthorPostsRequest {
        author_id: data.target.0,
        limit: 3,
    };
    let reply = ctx.posts().pinned_posts(ask).await;
    let reply = reply.map_err(ctx.failed(POSTS))?.into_inner();
    Ok(FeedPage {
        posts: reply.posts.into_iter().map(post_view).collect(),
        ..FeedPage::default()
    })
}

pub(super) async fn like_comment(
    ctx: Ctx,
    _: Principal,
    data: CommentRef,
) -> Result<CommentLikeView, ErrorBody> {
    let ask = CommentAction {
        comment_id: data.comment.0,
    };
    let reply = ctx.posts().like_comment(ask).await;
    let reply = reply.map_err(ctx.failed(POSTS))?.into_inner();
    Ok(CommentLikeView {
        comment: data.comment,
        changed: reply.changed,
        likes: reply.likes,
    })
}
