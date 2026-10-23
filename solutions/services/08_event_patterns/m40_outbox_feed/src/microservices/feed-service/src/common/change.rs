use contracts::{LinkView, PostId, UserId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    Registered {
        user: UserId,
        username: String,
    },
    Renamed {
        user: UserId,
        display_name: Option<String>,
    },
    Followed {
        follower: UserId,
        followee: UserId,
    },
    Muted {
        user: UserId,
        muted: UserId,
    },
    Posted {
        post: PostId,
        author: UserId,
        author_name: String,
        text: String,
        tags: Vec<String>,
        links: Vec<LinkView>,
        at: u64,
    },
    Liked {
        post: PostId,
    },
    Edited {
        post: PostId,
        text: String,
    },
    Deleted {
        post: PostId,
    },
    Nothing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Consumed {
    Applied,
    Buffered,
    Duplicate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedItem {
    pub post: PostId,
    pub author: UserId,
    pub author_name: String,
    pub text: String,
    pub likes: usize,
    pub tags: Vec<String>,
    pub links: Vec<LinkView>,
    pub at: u64,
}
