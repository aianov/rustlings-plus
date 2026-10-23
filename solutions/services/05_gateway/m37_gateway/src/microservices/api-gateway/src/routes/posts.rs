use contracts::{Changed, ErrorBody, FeedPage, PostId, PostView, Push, UserId};
use proto::social::v1::{
    CreatePostRequest, EditPostRequest, FeedRequest, PostAction, RelationsRequest,
};

use super::Ctx;
use crate::common::Principal;
use crate::dto::{EditPost, FeedAsk, NewPost, PostRef, links, post_view as view};

const POSTS: &str = "сервис постов";

async fn announce(ctx: &Ctx, author: UserId, post: PostId) {
    let ask = RelationsRequest { user_id: author.0 };
    match ctx.users().relations(ask).await {
        Ok(relations) => {
            for reader in relations.into_inner().audience {
                let to = UserId(reader);
                ctx.push(to, &Push::NewPost { to, author, post });
            }
        }
        Err(status) => eprintln!(
            "api-gateway [{}]: пуш о посте {post} не разослан, user-service ответил {status}",
            ctx.request_id()
        ),
    }
}

pub(super) async fn create(ctx: Ctx, me: Principal, data: NewPost) -> Result<PostView, ErrorBody> {
    let ask = CreatePostRequest {
        text: data.text,
        tags: data.tags,
        links: links(data.links),
    };
    let created = ctx.posts().create_post(ask).await;
    let post = view(created.map_err(ctx.failed(POSTS))?.into_inner());
    announce(&ctx, me.id, post.id).await;
    Ok(post)
}

pub(super) async fn like(ctx: Ctx, me: Principal, data: PostRef) -> Result<Changed, ErrorBody> {
    let post = data.post;
    let ask = PostAction { post_id: post.0 };
    let liked = ctx.posts().like(ask).await;
    let liked = liked.map_err(ctx.failed(POSTS))?.into_inner();
    let author = UserId(liked.author_id);
    if liked.changed && author != me.id {
        ctx.push(
            author,
            &Push::Liked {
                to: author,
                by: me.id,
                post,
            },
        );
    }
    Ok(Changed {
        changed: liked.changed,
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
    let feed = ctx.posts().feed(ask).await;
    let posts = feed.map_err(ctx.failed(POSTS))?.into_inner().posts;
    Ok(FeedPage {
        posts: posts.into_iter().map(view).collect(),
    })
}
