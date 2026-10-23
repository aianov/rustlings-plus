mod data;

use contracts::{
    FeedPush, Field, LinkView, MessageView, PostId, PostView, ProfilePatch, UserId, UserView,
};
use proto::social::v1::feed_event::Kind;
use proto::social::v1::{Link, Message, Post, Profile, UpdateProfileRequest};

pub use data::{
    EditPost, FeedAsk, FindUser, HistoryAsk, Letter, NewPost, Nothing, PostRef, Target, WatchAsk,
};

fn present(text: String) -> Option<String> {
    (!text.is_empty()).then_some(text)
}

pub fn post_view(post: Post) -> PostView {
    PostView {
        id: PostId(post.id),
        author: UserId(post.author_id),
        author_name: post.author_name,
        text: post.text,
        likes: usize::try_from(post.likes).unwrap_or(usize::MAX),
        tags: post.tags,
        links: post
            .links
            .into_iter()
            .map(|link| LinkView {
                url: link.url,
                title: present(link.title),
            })
            .collect(),
    }
}

pub fn links(links: Vec<LinkView>) -> Vec<Link> {
    links
        .into_iter()
        .map(|link| Link {
            url: link.url,
            title: link.title.unwrap_or_default(),
        })
        .collect()
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
        display_name: present(profile.display_name),
        bio: present(profile.bio),
    }
}

fn field(name: &str, field: &Field<String>, clear: &mut Vec<String>) -> Option<String> {
    match field {
        Field::Missing => None,
        Field::Null => {
            clear.push(name.to_string());
            None
        }
        Field::Value(value) => Some(value.clone()),
    }
}

pub fn profile_update(patch: &ProfilePatch) -> UpdateProfileRequest {
    let mut clear_fields = Vec::new();
    let display_name = field("display_name", &patch.display_name, &mut clear_fields);
    let bio = field("bio", &patch.bio, &mut clear_fields);
    UpdateProfileRequest {
        display_name,
        bio,
        clear_fields,
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
