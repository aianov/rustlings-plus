use crate::domain::DomainEvent;
use crate::ids::UserId;

pub const ROOT_ENV: &str = "EVENTS_ROOT";

pub const DEFAULT_ROOT: &str = "social";

pub const EVENTS: &str = "events";

pub const NOTICES: &str = "notices";

pub const NOTIFY: &str = "notify";

pub const DEAD: &str = "dead";

pub const STREAM_SUFFIX: &str = "_EVENTS";

pub const CHAT: &str = "chat";

pub const LIVE: &str = "live";

pub const TYPING: &str = "typing";

pub const SESSION: &str = "session";

pub const REVOKED: &str = "revoked";

pub const CHAT_SUFFIX: &str = "_CHAT";

pub const PRESENCE_SUFFIX: &str = "_PRESENCE";

pub const NOTIFY_GROUP: &str = "notify-service";

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

    pub fn event(&self, event: &DomainEvent) -> Vec<String> {
        words(&[
            &self.root,
            EVENTS,
            event.entity(),
            event.action(),
            &event.aggregate_id,
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

    pub fn chat_stream(&self) -> String {
        format!("{}{CHAT_SUFFIX}", self.root.to_ascii_uppercase())
    }

    pub fn chat_subjects(&self) -> Vec<String> {
        vec![self.chat_all().join(".")]
    }

    pub fn chat_all(&self) -> Vec<String> {
        words(&[&self.root, CHAT, ">"])
    }

    pub fn chat(&self, to: &str) -> Vec<String> {
        words(&[&self.root, CHAT, to])
    }

    pub fn live(&self, user: UserId) -> Vec<String> {
        let user = user.to_string();
        words(&[&self.root, LIVE, &user])
    }

    pub fn live_all(&self) -> Vec<String> {
        words(&[&self.root, LIVE, ">"])
    }

    pub fn typing(&self, user: UserId) -> Vec<String> {
        let user = user.to_string();
        words(&[&self.root, TYPING, &user])
    }

    pub fn revoked(&self) -> Vec<String> {
        words(&[&self.root, SESSION, REVOKED])
    }

    pub fn presence(&self) -> String {
        format!("{}{PRESENCE_SUFFIX}", self.root.to_ascii_uppercase())
    }
}
