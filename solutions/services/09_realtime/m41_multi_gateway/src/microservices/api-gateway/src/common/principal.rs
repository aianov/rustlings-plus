use std::time::{Duration, SystemTime, UNIX_EPOCH};

use contracts::{Role, UserId, UserView};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Principal {
    pub id: UserId,
    pub username: String,
    pub role: Role,
    pub session: String,
    pub expires_at: u64,
}

impl Principal {
    pub fn view(&self) -> UserView {
        UserView::named(self.id, &self.username)
    }

    pub fn left(&self, now: u64) -> Duration {
        Duration::from_secs(self.expires_at.saturating_sub(now))
    }

    pub fn is_expired(&self, now: u64) -> bool {
        now >= self.expires_at
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Visitor {
    Guest,
    Member(Principal),
}

impl Visitor {
    pub fn role(&self) -> Role {
        match self {
            Visitor::Guest => Role::Guest,
            Visitor::Member(principal) => principal.role,
        }
    }

    pub fn user(&self) -> Option<UserId> {
        match self {
            Visitor::Guest => None,
            Visitor::Member(principal) => Some(principal.id),
        }
    }
}

pub fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}
