use std::sync::Arc;

use contracts::{Cause, UserId, Username};
use journal::{Journal, JournalError};
use tokio::sync::{Mutex, Notify};

use super::entry::{entry_of, fact_of};
use super::state::State;
use super::{BoxFuture, RepoError, UserEntry, UserLine, UserRepository};
use crate::common::{ProfileChange, Relations, User};

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

    pub(super) async fn record(
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
                .filter(|user| !state.gone(user.id))
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
            let Some(index) = state.index(id).filter(|_| !state.gone(id)) else {
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
