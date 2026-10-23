mod views;

use serde::{Deserialize, Serialize};

use crate::chat::{DEFAULT_HISTORY_LIMIT, MAX_HISTORY_LIMIT};
use crate::command::{DEFAULT_FEED_LIMIT, ParseError};
use crate::envelope::{Envelope, service_of};
use crate::error::{Error, Result};
use crate::ids::{PostId, UserId};
use crate::username::Username;

pub use views::{
    Changed, ChatHistory, FeedPage, LinkView, MessageView, PostView, ProfileView, Reply, StatsView,
    UserView,
};

pub const METHOD_NAMES: [&str; 12] = [
    "users.register",
    "users.find",
    "users.follow",
    "users.mute",
    "users.profile",
    "posts.create",
    "posts.like",
    "posts.edit",
    "posts.delete",
    "posts.feed",
    "chat.send",
    "chat.history",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum Call {
    Register {
        username: Username,
    },
    FindUser {
        username: Username,
    },
    Follow {
        user: UserId,
        target: UserId,
    },
    Mute {
        user: UserId,
        target: UserId,
    },
    Profile {
        target: UserId,
    },
    CreatePost {
        user: UserId,
        text: String,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        tags: Vec<String>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        links: Vec<LinkView>,
    },
    Like {
        user: UserId,
        post: PostId,
    },
    EditPost {
        user: UserId,
        post: PostId,
        text: String,
    },
    DeletePost {
        user: UserId,
        post: PostId,
    },
    Feed {
        user: UserId,
        page: usize,
        size: usize,
    },
    SendMessage {
        user: UserId,
        peer: UserId,
        text: String,
    },
    History {
        user: UserId,
        peer: UserId,
        limit: usize,
    },
}

impl Call {
    pub fn method(&self) -> &'static str {
        match self {
            Call::Register { .. } => "users.register",
            Call::FindUser { .. } => "users.find",
            Call::Follow { .. } => "users.follow",
            Call::Mute { .. } => "users.mute",
            Call::Profile { .. } => "users.profile",
            Call::CreatePost { .. } => "posts.create",
            Call::Like { .. } => "posts.like",
            Call::EditPost { .. } => "posts.edit",
            Call::DeletePost { .. } => "posts.delete",
            Call::Feed { .. } => "posts.feed",
            Call::SendMessage { .. } => "chat.send",
            Call::History { .. } => "chat.history",
        }
    }
}

#[derive(Deserialize)]
struct NameData {
    username: Username,
}

#[derive(Deserialize)]
struct PairData {
    user: UserId,
    target: UserId,
}

#[derive(Deserialize)]
struct TargetData {
    target: UserId,
}

#[derive(Deserialize)]
struct DraftData {
    user: UserId,
    text: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    links: Vec<LinkView>,
}

#[derive(Deserialize)]
struct EditData {
    user: UserId,
    post: PostId,
    text: String,
}

#[derive(Deserialize)]
struct PostData {
    user: UserId,
    post: PostId,
}

#[derive(Deserialize)]
struct FeedData {
    user: UserId,
    #[serde(default = "first_page")]
    page: usize,
    #[serde(default = "feed_size")]
    size: usize,
}

#[derive(Deserialize)]
struct ChatData {
    user: UserId,
    peer: UserId,
    text: String,
}

#[derive(Deserialize)]
struct HistoryData {
    user: UserId,
    peer: UserId,
    #[serde(default = "history_limit")]
    limit: usize,
}

fn first_page() -> usize {
    1
}

fn feed_size() -> usize {
    DEFAULT_FEED_LIMIT
}

fn history_limit() -> usize {
    DEFAULT_HISTORY_LIMIT
}

fn data<'a, T: Deserialize<'a>>(envelope: &'a Envelope) -> Result<T> {
    T::deserialize(&envelope.data).map_err(|error| Error::bad_data(&envelope.method, error))
}

pub fn decode_call(envelope: &Envelope) -> Result<Call> {
    let method = envelope.method.as_str();
    if !METHOD_NAMES.contains(&method) {
        return Err(Error::UnknownMethod(method.to_string()));
    }
    if service_of(method) != envelope.service {
        return Err(Error::WrongService {
            service: envelope.service.to_string(),
            method: method.to_string(),
        });
    }
    match method {
        "users.register" => data(envelope).map(|d: NameData| Call::Register {
            username: d.username,
        }),
        "users.find" => data(envelope).map(|d: NameData| Call::FindUser {
            username: d.username,
        }),
        "users.follow" => data(envelope).map(|d: PairData| Call::Follow {
            user: d.user,
            target: d.target,
        }),
        "users.mute" => data(envelope).map(|d: PairData| Call::Mute {
            user: d.user,
            target: d.target,
        }),
        "users.profile" => data(envelope).map(|d: TargetData| Call::Profile { target: d.target }),
        "posts.create" => data(envelope).map(|d: DraftData| Call::CreatePost {
            user: d.user,
            text: d.text,
            tags: d.tags,
            links: d.links,
        }),
        "posts.like" => data(envelope).map(|d: PostData| Call::Like {
            user: d.user,
            post: d.post,
        }),
        "posts.edit" => data(envelope).map(|d: EditData| Call::EditPost {
            user: d.user,
            post: d.post,
            text: d.text,
        }),
        "posts.delete" => data(envelope).map(|d: PostData| Call::DeletePost {
            user: d.user,
            post: d.post,
        }),
        "posts.feed" => data(envelope).map(|d: FeedData| Call::Feed {
            user: d.user,
            page: d.page,
            size: d.size,
        }),
        "chat.send" => data(envelope).map(|d: ChatData| Call::SendMessage {
            user: d.user,
            peer: d.peer,
            text: d.text,
        }),
        "chat.history" => {
            let d: HistoryData = data(envelope)?;
            if !(1..=MAX_HISTORY_LIMIT).contains(&d.limit) {
                return Err(Error::bad_data(
                    method,
                    ParseError::LimitOutOfRange(d.limit),
                ));
            }
            Ok(Call::History {
                user: d.user,
                peer: d.peer,
                limit: d.limit,
            })
        }
        other => Err(Error::UnknownMethod(other.to_string())),
    }
}
