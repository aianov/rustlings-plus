use serde::{Deserialize, Serialize};

use crate::ids::{PostId, UserId};

pub const ROOT_ENV: &str = "EVENTS_ROOT";

pub const DEFAULT_ROOT: &str = "social";

pub const EVENTS: &str = "events";

pub const NOTICES: &str = "notices";

pub const NOTIFY: &str = "notify";

pub const DEAD: &str = "dead";

pub const STREAM_SUFFIX: &str = "_EVENTS";

pub const NOTIFY_GROUP: &str = "notify-service";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    UserFollowed,
    PostCreated,
    PostLiked,
    PostCommented,
}

impl EventKind {
    pub const ALL: [EventKind; 4] = [
        EventKind::UserFollowed,
        EventKind::PostCreated,
        EventKind::PostLiked,
        EventKind::PostCommented,
    ];

    pub fn entity(self) -> &'static str {
        match self {
            EventKind::UserFollowed => "user",
            EventKind::PostCreated | EventKind::PostLiked | EventKind::PostCommented => "post",
        }
    }

    pub fn action(self) -> &'static str {
        match self {
            EventKind::UserFollowed => "followed",
            EventKind::PostCreated => "created",
            EventKind::PostLiked => "liked",
            EventKind::PostCommented => "commented",
        }
    }

    pub fn of(entity: &str, action: &str) -> Option<EventKind> {
        EventKind::ALL
            .into_iter()
            .find(|kind| kind.entity() == entity && kind.action() == action)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventBody {
    pub actor: UserId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub post: Option<PostId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    pub kind: EventKind,
    pub owner: UserId,
    pub body: EventBody,
}

impl Event {
    pub fn followed(actor: UserId, target: UserId) -> Event {
        Event {
            kind: EventKind::UserFollowed,
            owner: target,
            body: EventBody { actor, post: None },
        }
    }

    pub fn post_created(author: UserId, post: PostId) -> Event {
        Event::about_post(EventKind::PostCreated, author, author, post)
    }

    pub fn post_liked(actor: UserId, author: UserId, post: PostId) -> Event {
        Event::about_post(EventKind::PostLiked, actor, author, post)
    }

    pub fn post_commented(actor: UserId, author: UserId, post: PostId) -> Event {
        Event::about_post(EventKind::PostCommented, actor, author, post)
    }

    fn about_post(kind: EventKind, actor: UserId, author: UserId, post: PostId) -> Event {
        Event {
            kind,
            owner: author,
            body: EventBody {
                actor,
                post: Some(post),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Origin {
    pub request_id: String,
    pub trace_id: Option<String>,
}

impl Origin {
    pub fn new(request_id: &str) -> Origin {
        Origin {
            request_id: request_id.to_string(),
            trace_id: None,
        }
    }

    pub fn traced(self, trace_id: Option<&str>) -> Origin {
        Origin {
            trace_id: trace_id.map(str::to_string),
            ..self
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NotifyMethod {
    Unread,
    List,
    Read,
}

impl NotifyMethod {
    pub const ALL: [NotifyMethod; 3] =
        [NotifyMethod::Unread, NotifyMethod::List, NotifyMethod::Read];

    pub fn name(self) -> &'static str {
        match self {
            NotifyMethod::Unread => "unread",
            NotifyMethod::List => "list",
            NotifyMethod::Read => "read",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Topics {
    root: String,
}

fn words(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|part| part.to_string()).collect()
}

impl Topics {
    pub fn new(root: &str) -> Topics {
        Topics {
            root: root.to_string(),
        }
    }

    pub fn root(&self) -> &str {
        &self.root
    }

    pub fn event(&self, event: &Event) -> Vec<String> {
        let owner = event.owner.to_string();
        words(&[
            &self.root,
            EVENTS,
            event.kind.entity(),
            event.kind.action(),
            &owner,
        ])
    }

    pub fn all_events(&self) -> Vec<String> {
        words(&[&self.root, EVENTS, ">"])
    }

    pub fn stream(&self) -> String {
        format!("{}{STREAM_SUFFIX}", self.root.to_ascii_uppercase())
    }

    pub fn stream_subjects(&self) -> Vec<String> {
        vec![
            words(&[&self.root, EVENTS, ">"]).join("."),
            words(&[&self.root, DEAD, ">"]).join("."),
        ]
    }

    pub fn dead(&self, consumer: &str) -> Vec<String> {
        words(&[&self.root, DEAD, consumer])
    }

    pub fn read_event(&self, subject: &str) -> Option<(EventKind, UserId)> {
        let parts: Vec<&str> = subject.split('.').collect();
        let [root, events, entity, action, owner] = parts.as_slice() else {
            return None;
        };
        if *root != self.root || *events != EVENTS {
            return None;
        }
        let kind = EventKind::of(entity, action)?;
        let parsed: UserId = owner.parse().ok()?;
        (parsed.to_string() == *owner).then_some((kind, parsed))
    }

    pub fn notice(&self, user: UserId) -> Vec<String> {
        let user = user.to_string();
        words(&[&self.root, NOTICES, &user])
    }

    pub fn all_notices(&self) -> Vec<String> {
        words(&[&self.root, NOTICES, "*"])
    }

    pub fn read_notice(&self, subject: &str) -> Option<UserId> {
        let parts: Vec<&str> = subject.split('.').collect();
        let [root, notices, user] = parts.as_slice() else {
            return None;
        };
        if *root != self.root || *notices != NOTICES {
            return None;
        }
        let parsed: UserId = user.parse().ok()?;
        (parsed.to_string() == *user).then_some(parsed)
    }

    pub fn notify(&self, method: NotifyMethod) -> Vec<String> {
        words(&[&self.root, NOTIFY, method.name()])
    }
}
