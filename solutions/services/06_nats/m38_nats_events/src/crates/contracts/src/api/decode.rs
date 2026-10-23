use serde::Deserialize;

use super::{Call, LinkView, METHOD_NAMES};
use crate::chat::{DEFAULT_HISTORY_LIMIT, MAX_HISTORY_LIMIT};
use crate::command::{DEFAULT_FEED_LIMIT, ParseError};
use crate::envelope::{Envelope, service_of};
use crate::error::{Error, Result};
use crate::ids::{PostId, UserId};
use crate::notice::{DEFAULT_NOTICES, MAX_NOTICES};
use crate::username::Username;

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
struct CommentData {
    user: UserId,
    post: PostId,
    text: String,
}

#[derive(Deserialize)]
struct UserData {
    user: UserId,
}

#[derive(Deserialize)]
struct NoticesData {
    user: UserId,
    #[serde(default = "notices_limit")]
    limit: u32,
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

fn notices_limit() -> u32 {
    DEFAULT_NOTICES
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
        "posts.comment" => data(envelope).map(|d: CommentData| Call::Comment {
            user: d.user,
            post: d.post,
            text: d.text,
        }),
        "notify.unread" => data(envelope).map(|d: UserData| Call::Unread { user: d.user }),
        "notify.read" => data(envelope).map(|d: UserData| Call::ReadNotices { user: d.user }),
        "notify.list" => {
            let d: NoticesData = data(envelope)?;
            if !(1..=MAX_NOTICES).contains(&d.limit) {
                return Err(Error::bad_data(
                    method,
                    ParseError::LimitOutOfRange(d.limit as usize),
                ));
            }
            Ok(Call::Notices {
                user: d.user,
                limit: d.limit,
            })
        }
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
