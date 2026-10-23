use proto::social::v1::feed_event::Kind;
use proto::social::v1::{CaughtUp, FeedEvent, Post as PostReply, Trend as TrendReply};

use crate::common::{Post, Trend};

pub fn post(post: &Post) -> PostReply {
    PostReply {
        id: post.id.0,
        author_id: post.author.0,
        author_name: post.author_name.clone(),
        text: post.text.clone(),
        likes: post.likes as u64,
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
