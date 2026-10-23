mod upcast;

use std::fmt;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::api::LinkView;
use crate::ids::{CommentId, PostId, UserId};

pub use upcast::upcast;

pub const PROFILE_SCHEMA: u32 = 2;

pub const KINDS: [&str; 9] = [
    "user.registered",
    "user.profile_changed",
    "user.followed",
    "user.muted",
    "post.created",
    "post.liked",
    "post.commented",
    "post.edited",
    "post.deleted",
];

fn first() -> u32 {
    1
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DomainEvent {
    pub event_id: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default = "first")]
    pub schema: u32,
    pub version: u64,
    pub occurred_at: u64,
    pub aggregate_id: String,
    #[serde(default)]
    pub payload: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Registered {
    pub user_id: UserId,
    pub username: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileChanged {
    pub user_id: UserId,
    #[serde(default)]
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Followed {
    pub follower_id: UserId,
    pub followee_id: UserId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Muted {
    pub user_id: UserId,
    pub muted_id: UserId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostCreated {
    pub post_id: PostId,
    pub author_id: UserId,
    pub author_name: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<LinkView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostLiked {
    pub post_id: PostId,
    pub author_id: UserId,
    pub user_id: UserId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostCommented {
    pub post_id: PostId,
    pub author_id: UserId,
    pub comment_id: CommentId,
    pub commenter_id: UserId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostEdited {
    pub post_id: PostId,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostDeleted {
    pub post_id: PostId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fact {
    Registered(Registered),
    ProfileChanged(ProfileChanged),
    Followed(Followed),
    Muted(Muted),
    PostCreated(PostCreated),
    PostLiked(PostLiked),
    PostCommented(PostCommented),
    PostEdited(PostEdited),
    PostDeleted(PostDeleted),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FactError {
    UnknownSchema { kind: String, schema: u32 },
    BadPayload { kind: String, reason: String },
}

impl fmt::Display for FactError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FactError::UnknownSchema { kind, schema } => {
                write!(f, "событие {kind} схемы {schema} читать не умею")
            }
            FactError::BadPayload { kind, reason } => write!(f, "событие {kind}: {reason}"),
        }
    }
}

impl std::error::Error for FactError {}

fn to_value<T: Serialize>(payload: &T) -> Value {
    serde_json::to_value(payload).unwrap_or(Value::Null)
}

fn read<T: DeserializeOwned>(kind: &str, payload: &Value) -> Result<T, FactError> {
    T::deserialize(payload).map_err(|error| FactError::BadPayload {
        kind: kind.to_string(),
        reason: error.to_string(),
    })
}

impl Fact {
    pub fn kind(&self) -> &'static str {
        match self {
            Fact::Registered(_) => KINDS[0],
            Fact::ProfileChanged(_) => KINDS[1],
            Fact::Followed(_) => KINDS[2],
            Fact::Muted(_) => KINDS[3],
            Fact::PostCreated(_) => KINDS[4],
            Fact::PostLiked(_) => KINDS[5],
            Fact::PostCommented(_) => KINDS[6],
            Fact::PostEdited(_) => KINDS[7],
            Fact::PostDeleted(_) => KINDS[8],
        }
    }

    pub fn schema(&self) -> u32 {
        match self {
            Fact::ProfileChanged(_) => PROFILE_SCHEMA,
            Fact::Registered(_)
            | Fact::Followed(_)
            | Fact::Muted(_)
            | Fact::PostCreated(_)
            | Fact::PostLiked(_)
            | Fact::PostCommented(_)
            | Fact::PostEdited(_)
            | Fact::PostDeleted(_) => 1,
        }
    }

    pub fn aggregate(&self) -> String {
        match self {
            Fact::Registered(fact) => fact.user_id.to_string(),
            Fact::ProfileChanged(fact) => fact.user_id.to_string(),
            Fact::Followed(fact) => fact.follower_id.to_string(),
            Fact::Muted(fact) => fact.user_id.to_string(),
            Fact::PostCreated(fact) => fact.post_id.to_string(),
            Fact::PostLiked(fact) => fact.post_id.to_string(),
            Fact::PostCommented(fact) => fact.post_id.to_string(),
            Fact::PostEdited(fact) => fact.post_id.to_string(),
            Fact::PostDeleted(fact) => fact.post_id.to_string(),
        }
    }

    pub fn payload(&self) -> Value {
        match self {
            Fact::Registered(fact) => to_value(fact),
            Fact::ProfileChanged(fact) => to_value(fact),
            Fact::Followed(fact) => to_value(fact),
            Fact::Muted(fact) => to_value(fact),
            Fact::PostCreated(fact) => to_value(fact),
            Fact::PostLiked(fact) => to_value(fact),
            Fact::PostCommented(fact) => to_value(fact),
            Fact::PostEdited(fact) => to_value(fact),
            Fact::PostDeleted(fact) => to_value(fact),
        }
    }

    pub fn parse(kind: &str, payload: &Value) -> Result<Option<Fact>, FactError> {
        let fact = match kind {
            "user.registered" => Fact::Registered(read(kind, payload)?),
            "user.profile_changed" => Fact::ProfileChanged(read(kind, payload)?),
            "user.followed" => Fact::Followed(read(kind, payload)?),
            "user.muted" => Fact::Muted(read(kind, payload)?),
            "post.created" => Fact::PostCreated(read(kind, payload)?),
            "post.liked" => Fact::PostLiked(read(kind, payload)?),
            "post.commented" => Fact::PostCommented(read(kind, payload)?),
            "post.edited" => Fact::PostEdited(read(kind, payload)?),
            "post.deleted" => Fact::PostDeleted(read(kind, payload)?),
            _unknown => return Ok(None),
        };
        Ok(Some(fact))
    }
}

impl DomainEvent {
    pub fn new(event_id: &str, fact: &Fact, version: u64, occurred_at: u64) -> DomainEvent {
        DomainEvent {
            event_id: event_id.to_string(),
            kind: fact.kind().to_string(),
            schema: fact.schema(),
            version,
            occurred_at,
            aggregate_id: fact.aggregate(),
            payload: fact.payload(),
        }
    }

    pub fn entity(&self) -> &str {
        self.kind
            .split_once('.')
            .map_or(&self.kind, |(entity, _)| entity)
    }

    pub fn action(&self) -> &str {
        self.kind.split_once('.').map_or("", |(_, action)| action)
    }

    pub fn fact(&self) -> Result<Option<Fact>, FactError> {
        if !KINDS.contains(&self.kind.as_str()) {
            return Ok(None);
        }
        let event = upcast(self.clone())?;
        Fact::parse(&event.kind, &event.payload)
    }
}
