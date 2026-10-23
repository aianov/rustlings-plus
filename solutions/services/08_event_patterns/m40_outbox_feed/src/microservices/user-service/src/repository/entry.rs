use contracts::{Fact, Followed, Muted, OutboxRow, ProfileChanged, Registered, UserId};
use serde::{Deserialize, Serialize};

use crate::common::User;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum UserEntry {
    Registered {
        id: u64,
        name: String,
    },
    Followed {
        who: u64,
        whom: u64,
    },
    Muted {
        who: u64,
        whom: u64,
    },
    Updated {
        id: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        display_name: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        bio: Option<String>,
    },
    Published {
        through: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserLine {
    #[serde(flatten)]
    pub entry: UserEntry,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outbox: Option<OutboxRow>,
}

pub(super) fn fact_of(entry: &UserEntry) -> Option<Fact> {
    let fact = match entry {
        UserEntry::Registered { id, name } => Fact::Registered(Registered {
            user_id: UserId(*id),
            username: name.clone(),
        }),
        UserEntry::Followed { who, whom } => Fact::Followed(Followed {
            follower_id: UserId(*who),
            followee_id: UserId(*whom),
        }),
        UserEntry::Muted { who, whom } => Fact::Muted(Muted {
            user_id: UserId(*who),
            muted_id: UserId(*whom),
        }),
        UserEntry::Updated {
            id, display_name, ..
        } => Fact::ProfileChanged(ProfileChanged {
            user_id: UserId(*id),
            display_name: display_name.clone(),
        }),
        UserEntry::Published { .. } => return None,
    };
    Some(fact)
}

pub(super) fn entry_of(user: &User) -> UserEntry {
    UserEntry::Updated {
        id: user.id.0,
        display_name: user
            .display_name
            .as_ref()
            .map(|name| name.as_ref().to_string()),
        bio: user.bio.as_ref().map(|bio| bio.as_ref().to_string()),
    }
}
