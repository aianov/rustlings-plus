use std::collections::{BTreeSet, HashMap};

use contracts::{UserId, Username};
use journal::{Journal, JournalError};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use super::{BoxFuture, RepoError, UserRepository};
use crate::common::{Relations, User};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum UserEntry {
    Registered { id: u64, name: String },
    Followed { who: u64, whom: u64 },
    Muted { who: u64, whom: u64 },
}

#[derive(Default)]
struct State {
    users: Vec<User>,
    by_name: HashMap<String, UserId>,
    following: HashMap<UserId, Vec<UserId>>,
    followers: HashMap<UserId, Vec<UserId>>,
    muted: HashMap<UserId, BTreeSet<UserId>>,
}

impl State {
    fn apply(&mut self, entry: UserEntry) -> Result<(), JournalError> {
        match entry {
            UserEntry::Registered { id, name } => {
                let name =
                    Username::try_from(name.as_str()).map_err(|error| JournalError::Broken {
                        line: self.users.len() + 1,
                        reason: error.to_string(),
                    })?;
                self.insert(UserId(id), name);
            }
            UserEntry::Followed { who, whom } => {
                self.add_follow(UserId(who), UserId(whom));
            }
            UserEntry::Muted { who, whom } => {
                self.muted
                    .entry(UserId(who))
                    .or_default()
                    .insert(UserId(whom));
            }
        }
        Ok(())
    }

    fn insert(&mut self, id: UserId, name: Username) -> User {
        let user = User { id, name };
        self.by_name.insert(user.name.as_ref().to_string(), id);
        self.users.push(user.clone());
        user
    }

    fn add_follow(&mut self, who: UserId, whom: UserId) -> bool {
        let list = self.following.entry(who).or_default();
        if list.contains(&whom) {
            return false;
        }
        list.push(whom);
        self.followers.entry(whom).or_default().push(who);
        true
    }

    fn user(&self, id: UserId) -> Option<User> {
        let index = usize::try_from(id.0).ok()?.checked_sub(1)?;
        self.users.get(index).cloned()
    }
}

pub struct MemoryUsers {
    state: Mutex<State>,
    journal: Journal<UserEntry>,
}

impl MemoryUsers {
    pub fn new() -> MemoryUsers {
        MemoryUsers {
            state: Mutex::default(),
            journal: Journal::memory(),
        }
    }

    pub fn restore(
        journal: Journal<UserEntry>,
        entries: Vec<UserEntry>,
    ) -> Result<MemoryUsers, JournalError> {
        let mut state = State::default();
        for entry in entries {
            state.apply(entry)?;
        }
        Ok(MemoryUsers {
            state: Mutex::new(state),
            journal,
        })
    }
}

impl Default for MemoryUsers {
    fn default() -> MemoryUsers {
        MemoryUsers::new()
    }
}

impl UserRepository for MemoryUsers {
    fn create_if_absent(&self, name: Username) -> BoxFuture<'_, Result<User, RepoError>> {
        Box::pin(async move {
            let mut state = self.state.lock().await;
            if state.by_name.contains_key(name.as_ref()) {
                return Err(RepoError::Conflict);
            }
            let id = UserId(state.users.len() as u64 + 1);
            let entry = UserEntry::Registered {
                id: id.0,
                name: name.as_ref().to_string(),
            };
            self.journal.append(&entry).await?;
            Ok(state.insert(id, name))
        })
    }

    fn get(&self, id: UserId) -> BoxFuture<'_, Option<User>> {
        Box::pin(async move { self.state.lock().await.user(id) })
    }

    fn find(&self, name: Username) -> BoxFuture<'_, Option<User>> {
        Box::pin(async move {
            let state = self.state.lock().await;
            let id = *state.by_name.get(name.as_ref())?;
            state.user(id)
        })
    }

    fn list_after(&self, after: u64, limit: usize) -> BoxFuture<'_, Vec<User>> {
        Box::pin(async move {
            let state = self.state.lock().await;
            let start = usize::try_from(after).unwrap_or(usize::MAX);
            state
                .users
                .iter()
                .skip(start)
                .take(limit)
                .cloned()
                .collect()
        })
    }

    fn follow(&self, who: UserId, whom: UserId) -> BoxFuture<'_, Result<bool, JournalError>> {
        Box::pin(async move {
            let mut state = self.state.lock().await;
            if state
                .following
                .get(&who)
                .is_some_and(|list| list.contains(&whom))
            {
                return Ok(false);
            }
            let entry = UserEntry::Followed {
                who: who.0,
                whom: whom.0,
            };
            self.journal.append(&entry).await?;
            Ok(state.add_follow(who, whom))
        })
    }

    fn mute(&self, who: UserId, whom: UserId) -> BoxFuture<'_, Result<bool, JournalError>> {
        Box::pin(async move {
            let mut state = self.state.lock().await;
            if state
                .muted
                .get(&who)
                .is_some_and(|hidden| hidden.contains(&whom))
            {
                return Ok(false);
            }
            let entry = UserEntry::Muted {
                who: who.0,
                whom: whom.0,
            };
            self.journal.append(&entry).await?;
            Ok(state.muted.entry(who).or_default().insert(whom))
        })
    }

    fn relations(&self, id: UserId) -> BoxFuture<'_, Relations> {
        Box::pin(async move {
            let state = self.state.lock().await;
            let followers = state.followers.get(&id).cloned().unwrap_or_default();
            let audience = followers
                .iter()
                .copied()
                .filter(|follower| !state.muted.get(follower).is_some_and(|m| m.contains(&id)))
                .collect();
            Relations {
                following: state.following.get(&id).cloned().unwrap_or_default(),
                followers,
                muted: state
                    .muted
                    .get(&id)
                    .map(|hidden| hidden.iter().copied().collect())
                    .unwrap_or_default(),
                audience,
            }
        })
    }
}
