use contracts::{FeedPush, MessageView, PostId, PostView, UserId, UserView};
use proto::social::v1::feed_event::Kind;
use proto::social::v1::{Message, Post, Profile};

pub fn post_view(post: Post) -> PostView {
    PostView {
        id: PostId(post.id),
        author: UserId(post.author_id),
        author_name: post.author_name,
        text: post.text,
        likes: usize::try_from(post.likes).unwrap_or(usize::MAX),
    }
}

pub fn message_view(message: Message) -> MessageView {
    MessageView {
        from: UserId(message.from_id),
        to: UserId(message.to_id),
        text: message.text,
    }
}

pub fn user_view(profile: Profile) -> UserView {
    UserView {
        id: UserId(profile.id),
        username: profile.username,
    }
}

pub fn feed_event(to: UserId, kind: Kind) -> FeedPush {
    match kind {
        Kind::History(post) => FeedPush::FeedPost {
            to,
            post: post_view(post),
            live: false,
        },
        Kind::CaughtUp(..) => FeedPush::FeedCaughtUp { to },
        Kind::Live(post) => FeedPush::FeedPost {
            to,
            post: post_view(post),
            live: true,
        },
    }
}
