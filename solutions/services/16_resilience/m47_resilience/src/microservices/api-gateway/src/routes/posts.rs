use contracts::{Changed, CommentId, CommentView, ErrorBody, FeedPage, PostView};
use proto::social::v1::{
    CommentRequest, CreatePostRequest, EditPostRequest, FeedRequest, PostAction,
};

use super::Ctx;
use crate::common::Principal;
use crate::dto::{CommentAsk, EditPost, FeedAsk, NewPost, PostRef, links, post_view as view};

const POSTS: &str = "сервис постов";

const FEED: &str = "сервис ленты";

pub(super) async fn create(ctx: Ctx, _: Principal, data: NewPost) -> Result<PostView, ErrorBody> {
    let ask = CreatePostRequest {
        text: data.text,
        tags: data.tags,
        links: links(data.links),
        idempotency_key: data.idempotency_key.unwrap_or_default(),
    };
    let created = ctx.posts().create_post(ask).await;
    Ok(view(created.map_err(ctx.failed(POSTS))?.into_inner()))
}

pub(super) async fn like(ctx: Ctx, _: Principal, data: PostRef) -> Result<Changed, ErrorBody> {
    let ask = PostAction {
        post_id: data.post.0,
    };
    let liked = ctx.posts().like(ask).await;
    let liked = liked.map_err(ctx.failed(POSTS))?.into_inner();
    Ok(Changed {
        changed: liked.changed,
    })
}

pub(super) async fn comment(
    ctx: Ctx,
    _: Principal,
    data: CommentAsk,
) -> Result<CommentView, ErrorBody> {
    let ask = CommentRequest {
        post_id: data.post.0,
        text: data.text,
    };
    let commented = ctx.posts().comment(ask).await;
    let reply = commented.map_err(ctx.failed(POSTS))?.into_inner();
    Ok(CommentView {
        id: CommentId(reply.comment_id),
        post: data.post,
    })
}

pub(super) async fn edit(ctx: Ctx, _: Principal, data: EditPost) -> Result<PostView, ErrorBody> {
    let ask = EditPostRequest {
        post_id: data.post.0,
        text: data.text,
    };
    let edited = ctx.posts().edit_post(ask).await;
    Ok(view(edited.map_err(ctx.failed(POSTS))?.into_inner()))
}

pub(super) async fn delete(ctx: Ctx, _: Principal, data: PostRef) -> Result<Changed, ErrorBody> {
    let ask = PostAction {
        post_id: data.post.0,
    };
    let deleted = ctx.posts().delete_post(ask).await;
    let changed = deleted.map_err(ctx.failed(POSTS))?.into_inner().changed;
    Ok(Changed { changed })
}

pub(super) async fn feed(ctx: Ctx, _: Principal, data: FeedAsk) -> Result<FeedPage, ErrorBody> {
    let ask = FeedRequest {
        page: u32::try_from(data.page).unwrap_or(u32::MAX),
        size: u32::try_from(data.size).unwrap_or(u32::MAX),
    };
    let feed = ctx.feed().page(ask).await;
    let posts: Vec<PostView> = feed
        .map_err(ctx.failed(FEED))?
        .into_inner()
        .posts
        .into_iter()
        .map(view)
        .collect();
    let (authors, stale) = super::profiles::authors_of(&ctx, &posts).await;
    Ok(FeedPage {
        posts,
        authors,
        stale,
    })
}
