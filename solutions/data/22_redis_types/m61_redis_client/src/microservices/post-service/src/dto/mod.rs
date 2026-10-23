use contracts::UserId;
use proto::social::v1::feed_event::Kind;
use proto::social::v1::{
    AuthorPostsRequest, AuthorRank, CaughtUp, CommentRequest, CreatePostRequest, EditPostRequest,
    FeedEvent, LatestComment, Link as LinkReply, Post as PostReply, ThreadNode,
    Trend as TrendReply,
};
use validation::{
    CharLen, HttpUrl, Invalid, MaxItems, NoControl, Range, RuleExt, Unique, Validator,
};

use crate::common::{
    CommentText, Draft, Link, MAX_AUTHOR_POSTS, MAX_LINK_TITLE, MAX_LINKS, MAX_TAGS, Post,
    PostText, Tag, Trend,
};
use crate::repository::{Line, Node, Rank};

fn link(check: &mut Validator, raw: &LinkReply) -> Option<Link> {
    let url = raw.url.trim().to_string();
    let url_ok = check.check("url", url.as_str(), &HttpUrl);
    let title = raw.title.trim();
    let title_rule = NoControl.and(CharLen {
        min: 0,
        max: MAX_LINK_TITLE,
    });
    let title_ok = check.check("title", title, &title_rule);
    (url_ok && title_ok).then(|| Link {
        url,
        title: (!title.is_empty()).then(|| title.to_string()),
    })
}

pub fn draft(request: &CreatePostRequest) -> Result<Draft, Invalid> {
    let mut check = Validator::new();
    let text = check.parse::<PostText>("text", &request.text);
    let mut tags = Vec::new();
    if check.check("tags", request.tags.as_slice(), &MaxItems(MAX_TAGS)) {
        check.each("tags", &request.tags, |check, raw| {
            tags.extend(check.parse::<Tag>("", raw));
        });
        if tags.len() == request.tags.len() {
            check.check("tags", tags.as_slice(), &Unique);
        }
    }
    let mut links = Vec::new();
    if check.check("links", request.links.as_slice(), &MaxItems(MAX_LINKS)) {
        check.each("links", &request.links, |check, raw| {
            links.extend(link(check, raw));
        });
    }
    let text = check.finish_with(text)?;
    Ok(Draft { text, tags, links })
}

pub fn edit_text(request: &EditPostRequest) -> Result<PostText, Invalid> {
    let mut check = Validator::new();
    let text = check.parse::<PostText>("text", &request.text);
    check.finish_with(text)
}

pub fn author_page(request: &AuthorPostsRequest) -> Result<(UserId, usize), Invalid> {
    let mut check = Validator::new();
    let limit = usize::try_from(request.limit).unwrap_or(usize::MAX);
    let range = Range {
        min: 1,
        max: MAX_AUTHOR_POSTS,
    };
    let fits = check.check("limit", &limit, &range);
    check.finish_with(fits.then_some((UserId(request.author_id), limit)))
}

pub fn comment_text(request: &CommentRequest) -> Result<CommentText, Invalid> {
    let mut check = Validator::new();
    let text = check.parse::<CommentText>("text", &request.text);
    check.finish_with(text)
}

pub fn marked(post: &Post, liked: bool) -> PostReply {
    PostReply {
        liked,
        ..self::post(post)
    }
}

pub fn post(post: &Post) -> PostReply {
    PostReply {
        liked: false,
        latest: Vec::new(),
        views: 0,
        id: post.id.0,
        author_id: post.author.0,
        author_name: post.author_name.clone(),
        text: post.text.clone(),
        likes: post.likes as u64,
        tags: post.tags.clone(),
        links: post
            .links
            .iter()
            .map(|link| LinkReply {
                url: link.url.clone(),
                title: link.title.clone().unwrap_or_default(),
            })
            .collect(),
    }
}

pub fn trend(trend: &Trend) -> TrendReply {
    TrendReply {
        post_id: trend.post.0,
        likes: trend.likes as u64,
    }
}

pub fn history(post_view: &Post) -> FeedEvent {
    FeedEvent {
        kind: Some(Kind::History(post(post_view))),
    }
}

pub fn caught_up() -> FeedEvent {
    FeedEvent {
        kind: Some(Kind::CaughtUp(CaughtUp {})),
    }
}

pub fn live(post_view: &Post) -> FeedEvent {
    FeedEvent {
        kind: Some(Kind::Live(post(post_view))),
    }
}

pub fn line(line: Line) -> LatestComment {
    LatestComment {
        id: line.id.0,
        author_id: line.author.0,
        text: line.text,
    }
}

pub fn node(node: Node) -> ThreadNode {
    ThreadNode {
        id: node.id.0,
        parent_id: node.parent.map_or(0, |parent| parent.0),
        author_id: node.author.0,
        text: node.text,
        depth: node.depth,
        replies: node.replies,
        deleted: node.deleted,
    }
}

pub fn rank(rank: Rank) -> AuthorRank {
    AuthorRank {
        author_id: rank.author.0,
        author_name: rank.author_name,
        place: rank.place,
        likes: rank.likes,
        shift: rank.shift,
    }
}
