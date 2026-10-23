use serde::{Deserialize, Serialize};

use crate::cursor::Timestamp;
use crate::ids::{CommentId, PostId, UserId};
use crate::visibility::Visibility;

pub const ENTRY_EVENTS: [&str; 9] = [
    "registered",
    "followed",
    "muted",
    "posted",
    "liked",
    "deleted",
    "commented",
    "replied",
    "message_sent",
];

/// ```
/// use contracts::{Entry, UserId, decode};
///
/// let entry: Entry = decode(r#"{"event": "followed", "who": "u_2", "whom": "u_1"}"#)?;
/// assert_eq!(entry, Entry::Followed { who: UserId(2), whom: UserId(1) });
/// assert_eq!(entry.event(), "followed");
/// # Ok::<(), contracts::Error>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Entry {
    Registered {
        user: UserId,
        name: String,
    },
    Followed {
        who: UserId,
        whom: UserId,
    },
    Muted {
        who: UserId,
        whom: UserId,
    },
    Posted {
        post: PostId,
        author: UserId,
        text: String,
        visibility: Visibility,
        at: Timestamp,
    },
    Liked {
        post: PostId,
        by: UserId,
        at: Timestamp,
    },
    Deleted {
        post: PostId,
        by: UserId,
    },
    Commented {
        comment: CommentId,
        post: PostId,
        author: UserId,
        text: String,
    },
    Replied {
        comment: CommentId,
        parent: CommentId,
        author: UserId,
        text: String,
    },
    MessageSent {
        from: UserId,
        to: UserId,
        text: String,
    },
}

impl Entry {
    pub fn event(&self) -> &'static str {
        match self {
            Entry::Registered { .. } => ENTRY_EVENTS[0],
            Entry::Followed { .. } => ENTRY_EVENTS[1],
            Entry::Muted { .. } => ENTRY_EVENTS[2],
            Entry::Posted { .. } => ENTRY_EVENTS[3],
            Entry::Liked { .. } => ENTRY_EVENTS[4],
            Entry::Deleted { .. } => ENTRY_EVENTS[5],
            Entry::Commented { .. } => ENTRY_EVENTS[6],
            Entry::Replied { .. } => ENTRY_EVENTS[7],
            Entry::MessageSent { .. } => ENTRY_EVENTS[8],
        }
    }
}
