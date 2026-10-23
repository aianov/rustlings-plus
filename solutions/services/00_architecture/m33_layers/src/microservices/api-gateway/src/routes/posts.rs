use contracts::{Call, Changed, Envelope, FeedPage, PostId, Push, Reply, UserId, UserView};
use proto::social::v1::{CreatePostRequest, FeedRequest, PostAction, RelationsRequest};

use super::{Answer, Ctx, acting, misrouted};
use crate::dto::post_view as view;

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

pub(super) async fn create(ctx: Ctx, me: UserView, envelope: Envelope) -> Answer {
    let call = acting(&me, envelope)?;
    let Call::CreatePost { user, text } = call else {
        return Err(misrouted(call.method()));
    };
    let ask = CreatePostRequest { text };
    let created = ctx.posts().create_post(ask).await;
    let post = view(created.map_err(ctx.failed(POSTS))?.into_inner());
    announce(&ctx, user, post.id).await;
    Ok(Reply::Post(post))
}

pub(super) async fn like(ctx: Ctx, me: UserView, envelope: Envelope) -> Answer {
    let call = acting(&me, envelope)?;
    let Call::Like { user, post } = call else {
        return Err(misrouted(call.method()));
    };
    let ask = PostAction { post_id: post.0 };
    let liked = ctx.posts().like(ask).await;
    let liked = liked.map_err(ctx.failed(POSTS))?.into_inner();
    let author = UserId(liked.author_id);
    if liked.changed && author != user {
        ctx.push(
            author,
            &Push::Liked {
                to: author,
                by: user,
                post,
            },
        );
    }
    Ok(Reply::Changed(Changed {
        changed: liked.changed,
    }))
}

pub(super) async fn delete(ctx: Ctx, me: UserView, envelope: Envelope) -> Answer {
    let call = acting(&me, envelope)?;
    let Call::DeletePost { post, .. } = call else {
        return Err(misrouted(call.method()));
    };
    let ask = PostAction { post_id: post.0 };
    let deleted = ctx.posts().delete_post(ask).await;
    let changed = deleted.map_err(ctx.failed(POSTS))?.into_inner().changed;
    Ok(Reply::Changed(Changed { changed }))
}

pub(super) async fn feed(ctx: Ctx, me: UserView, envelope: Envelope) -> Answer {
    let call = acting(&me, envelope)?;
    let Call::Feed { page, size, .. } = call else {
        return Err(misrouted(call.method()));
    };
    let ask = FeedRequest {
        page: u32::try_from(page).unwrap_or(u32::MAX),
        size: u32::try_from(size).unwrap_or(u32::MAX),
    };
    let feed = ctx.posts().feed(ask).await;
    let posts = feed.map_err(ctx.failed(POSTS))?.into_inner().posts;
    Ok(Reply::Feed(FeedPage {
        posts: posts.into_iter().map(view).collect(),
    }))
}
