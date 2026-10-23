use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use contracts::{Cause, Outbox, UserId, Username};
use journal::{Journal, JournalError};
use tokio::sync::{Mutex, Notify};

use super::entry::{entry_of, fact_of};
use super::{BoxFuture, RepoError, UserEntry, UserLine, UserRepository};
use crate::common::{Bio, DisplayName, ProfileChange, Relations, User};

#[derive(Default)]
pub(super) struct State {
    users: Vec<User>,
    by_name: HashMap<String, UserId>,
    following: HashMap<UserId, Vec<UserId>>,
    followers: HashMap<UserId, Vec<UserId>>,
    muted: HashMap<UserId, BTreeSet<UserId>>,
    pub(super) outbox: Outbox,
}

impl State {
    fn apply(&mut self, entry: UserEntry, line: usize) -> Result<(), JournalError> {
        let broken = |reason: String| JournalError::Broken { line, reason };
        match entry {
            UserEntry::Registered { id, name } => {
                let name =
                    Username::try_from(name.as_str()).map_err(|error| broken(error.to_string()))?;
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
            UserEntry::Updated {
                id,
                display_name,
                bio,
            } => {
                let display_name = display_name
                    .map(|raw| DisplayName::try_from(raw.as_str()))
                    .transpose()
                    .map_err(|error| broken(error.to_string()))?;
                let bio = bio
                    .map(|raw| Bio::try_from(raw.as_str()))
                    .transpose()
                    .map_err(|error| broken(error.to_string()))?;
                let index = self
                    .index(UserId(id))
                    .ok_or_else(|| broken(format!("профиля {id} нет")))?;
                self.users[index].display_name = display_name;
                self.users[index].bio = bio;
            }
            UserEntry::Published { through } => self.outbox.published_through(through),
        }
        Ok(())
    }

    fn index(&self, id: UserId) -> Option<usize> {
        let index = usize::try_from(id.0).ok()?.checked_sub(1)?;
        (index < self.users.len()).then_some(index)
    }

    fn insert(&mut self, id: UserId, name: Username) -> User {
        let user = User::new(id, name);
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
        self.users.get(self.index(id)?).cloned()
    }
}

pub struct MemoryUsers {
    pub(super) state: Mutex<State>,
    pub(super) journal: Journal<UserLine>,
    pub(super) signal: Arc<Notify>,
}

impl MemoryUsers {
    pub fn new() -> MemoryUsers {
        MemoryUsers {
            state: Mutex::default(),
            journal: Journal::memory(),
            signal: Arc::default(),
        }
    }

    pub fn restore(
        journal: Journal<UserLine>,
        lines: Vec<UserLine>,
    ) -> Result<MemoryUsers, JournalError> {
        let mut state = State::default();
        for (index, line) in lines.into_iter().enumerate() {
            state.apply(line.entry, index + 1)?;
            if let Some(row) = line.outbox {
                state.outbox.push(row);
            }
        }
        Ok(MemoryUsers {
            state: Mutex::new(state),
            journal,
            signal: Arc::default(),
        })
    }

    async fn record(
        &self,
        state: &mut State,
        entry: UserEntry,
        cause: &Cause,
    ) -> Result<(), JournalError> {
        let row = fact_of(&entry).map(|fact| state.outbox.prepare(&fact, cause));
        let line = UserLine {
            entry,
            outbox: row.clone(),
        };
        self.journal.append(&line).await?;
        if let Some(row) = row {
            state.outbox.push(row);
            self.signal.notify_one();
        }
        Ok(())
    }
}

impl Default for MemoryUsers {
    fn default() -> MemoryUsers {
        MemoryUsers::new()
    }
}

impl UserRepository for MemoryUsers {
    fn create_if_absent(
        &self,
        name: Username,
        cause: &Cause,
    ) -> BoxFuture<'_, Result<User, RepoError>> {
        let cause = cause.clone();
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
            self.record(&mut state, entry, &cause).await?;
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

    fn follow(
        &self,
        who: UserId,
        whom: UserId,
        cause: &Cause,
    ) -> BoxFuture<'_, Result<bool, JournalError>> {
        let cause = cause.clone();
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
            self.record(&mut state, entry, &cause).await?;
            Ok(state.add_follow(who, whom))
        })
    }

    fn mute(
        &self,
        who: UserId,
        whom: UserId,
        cause: &Cause,
    ) -> BoxFuture<'_, Result<bool, JournalError>> {
        let cause = cause.clone();
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
            self.record(&mut state, entry, &cause).await?;
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

    fn update(
        &self,
        id: UserId,
        change: ProfileChange,
        cause: &Cause,
    ) -> BoxFuture<'_, Result<Option<User>, JournalError>> {
        let cause = cause.clone();
        Box::pin(async move {
            let mut state = self.state.lock().await;
            let Some(index) = state.index(id) else {
                return Ok(None);
            };
            let next = change.apply(&state.users[index]);
            if next != state.users[index] {
                self.record(&mut state, entry_of(&next), &cause).await?;
                state.users[index] = next.clone();
            }
            Ok(Some(next))
        })
    }
}
