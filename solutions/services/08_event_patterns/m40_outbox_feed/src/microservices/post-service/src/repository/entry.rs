use contracts::{
    CommentId, Fact, LinkView, OutboxRow, PostCommented, PostCreated, PostDeleted, PostEdited,
    PostId, PostLiked, UserId,
};
use serde::{Deserialize, Serialize};

use super::state::State;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum PostEntry {
    Posted {
        id: u64,
        author: u64,
        author_name: String,
        text: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        tags: Vec<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        links: Vec<LinkEntry>,
        at: u64,
    },
    Liked {
        post: u64,
        by: u64,
        at: u64,
    },
    Edited {
        post: u64,
        text: String,
    },
    Deleted {
        post: u64,
    },
    Commented {
        id: u64,
        post: u64,
        author: u64,
        text: String,
    },
    Published {
        through: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PostLine {
    #[serde(flatten)]
    pub entry: PostEntry,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outbox: Option<OutboxRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkEntry {
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

pub(super) fn fact_of(state: &State, entry: &PostEntry) -> Option<Fact> {
    let author_of = |post: u64| state.post(PostId(post)).map(|found| found.author);
    let fact = match entry {
        PostEntry::Posted {
            id,
            author,
            author_name,
            text,
            tags,
            links,
            ..
        } => Fact::PostCreated(PostCreated {
            post_id: PostId(*id),
            author_id: UserId(*author),
            author_name: author_name.clone(),
            text: text.clone(),
            tags: tags.clone(),
            links: links
                .iter()
                .map(|link| LinkView {
                    url: link.url.clone(),
                    title: link.title.clone(),
                })
                .collect(),
        }),
        PostEntry::Liked { post, by, .. } => Fact::PostLiked(PostLiked {
            post_id: PostId(*post),
            author_id: author_of(*post)?,
            user_id: UserId(*by),
        }),
        PostEntry::Edited { post, text } => Fact::PostEdited(PostEdited {
            post_id: PostId(*post),
            text: text.clone(),
        }),
        PostEntry::Deleted { post } => Fact::PostDeleted(PostDeleted {
            post_id: PostId(*post),
        }),
        PostEntry::Commented {
            id, post, author, ..
        } => Fact::PostCommented(PostCommented {
            post_id: PostId(*post),
            author_id: author_of(*post)?,
            comment_id: CommentId(*id),
            commenter_id: UserId(*author),
        }),
        PostEntry::Published { .. } => return None,
    };
    Some(fact)
}
