use contracts::{
    CommentId, CommentNodeView, ErrorBody, FeedPage, PeopleView, RankView, ThreadView, TopView,
    UserId,
};
use proto::social::v1::{RelatedRequest, SearchRequest, ThreadRequest, TopRequest};

use super::Ctx;
use crate::common::Principal;
use crate::dto::{SearchAsk, SuggestAsk, ThreadAsk, TopAsk, post_view};

const POSTS: &str = "сервис постов";

pub(super) async fn thread(ctx: Ctx, data: ThreadAsk) -> Result<ThreadView, ErrorBody> {
    let ask = ThreadRequest {
        post_id: data.post.0,
        max_depth: data.depth,
    };
    let reply = ctx.posts().thread(ask).await;
    let reply = reply.map_err(ctx.failed(POSTS))?.into_inner();
    Ok(ThreadView {
        post: data.post,
        nodes: reply
            .nodes
            .into_iter()
            .map(|node| CommentNodeView {
                id: CommentId(node.id),
                parent: (node.parent_id > 0).then_some(CommentId(node.parent_id)),
                author: UserId(node.author_id),
                text: node.text,
                depth: node.depth,
                replies: node.replies,
                deleted: node.deleted,
            })
            .collect(),
    })
}

pub(super) async fn top(ctx: Ctx, data: TopAsk) -> Result<TopView, ErrorBody> {
    let reply = ctx
        .posts()
        .top_authors(TopRequest { limit: data.limit })
        .await;
    let reply = reply.map_err(ctx.failed(POSTS))?.into_inner();
    Ok(TopView {
        authors: reply
            .authors
            .into_iter()
            .map(|rank| RankView {
                author: UserId(rank.author_id),
                author_name: rank.author_name,
                place: rank.place,
                likes: rank.likes,
                shift: rank.shift,
            })
            .collect(),
    })
}

pub(super) async fn suggest(
    ctx: Ctx,
    me: Principal,
    data: SuggestAsk,
) -> Result<PeopleView, ErrorBody> {
    let ask = RelatedRequest {
        user_id: me.id.0,
        after: 0,
        limit: data.limit,
    };
    let found = ctx.users().suggest(ask).await;
    super::social::people(&ctx, data.limit, found)
}

pub(super) async fn search(ctx: Ctx, data: SearchAsk) -> Result<FeedPage, ErrorBody> {
    let ask = SearchRequest {
        author_id: data.author.map_or(0, |author| author.0),
        tag: data.tag.unwrap_or_default(),
        text: data.text.unwrap_or_default(),
        since: data.since.unwrap_or_default(),
        sort: data.sort.unwrap_or_default(),
        limit: data.limit,
    };
    let reply = ctx.posts().search_posts(ask).await;
    let reply = reply.map_err(ctx.failed(POSTS))?.into_inner();
    Ok(FeedPage {
        posts: reply.posts.into_iter().map(post_view).collect(),
        ..FeedPage::default()
    })
}
