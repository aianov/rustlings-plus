use std::collections::{BTreeSet, HashMap};

use contracts::{Outbox, UserId, Username};
use journal::JournalError;

use super::UserEntry;
use crate::common::{Bio, DisplayName, User};

#[derive(Default)]
pub(super) struct State {
    pub(super) users: Vec<User>,
    pub(super) by_name: HashMap<String, UserId>,
    pub(super) following: HashMap<UserId, Vec<UserId>>,
    pub(super) followers: HashMap<UserId, Vec<UserId>>,
    pub(super) muted: HashMap<UserId, BTreeSet<UserId>>,
    pub(super) hidden: BTreeSet<UserId>,
    pub(super) deleted: BTreeSet<UserId>,
    pub(super) outbox: Outbox,
}

impl State {
    pub(super) fn apply(&mut self, entry: UserEntry, line: usize) -> Result<(), JournalError> {
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
            UserEntry::Hidden { id } => {
                self.hidden.insert(UserId(id));
            }
            UserEntry::Shown { id } => {
                self.hidden.remove(&UserId(id));
            }
            UserEntry::Deleted { id } => self.erase(UserId(id)),
            UserEntry::Published { through } => self.outbox.published_through(through),
        }
        Ok(())
    }

    pub(super) fn index(&self, id: UserId) -> Option<usize> {
        let index = usize::try_from(id.0).ok()?.checked_sub(1)?;
        (index < self.users.len()).then_some(index)
    }

    pub(super) fn insert(&mut self, id: UserId, name: Username) -> User {
        let user = User::new(id, name);
        self.by_name.insert(user.name.as_ref().to_string(), id);
        self.users.push(user.clone());
        user
    }

    pub(super) fn add_follow(&mut self, who: UserId, whom: UserId) -> bool {
        let list = self.following.entry(who).or_default();
        if list.contains(&whom) {
            return false;
        }
        list.push(whom);
        self.followers.entry(whom).or_default().push(who);
        true
    }

    pub(super) fn gone(&self, id: UserId) -> bool {
        self.hidden.contains(&id) || self.deleted.contains(&id)
    }

    pub(super) fn user(&self, id: UserId) -> Option<User> {
        if self.gone(id) {
            return None;
        }
        self.users.get(self.index(id)?).cloned()
    }

    pub(super) fn erase(&mut self, id: UserId) {
        if let Some(index) = self.index(id) {
            self.by_name.remove(self.users[index].name.as_ref());
        }
        self.hidden.remove(&id);
        self.deleted.insert(id);
        for list in self
            .following
            .values_mut()
            .chain(self.followers.values_mut())
        {
            list.retain(|other| *other != id);
        }
        self.following.remove(&id);
        self.followers.remove(&id);
        for hidden in self.muted.values_mut() {
            hidden.remove(&id);
        }
        self.muted.remove(&id);
    }
}
