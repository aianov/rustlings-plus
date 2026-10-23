use std::collections::{HashMap, HashSet};

use crate::domain::UserId;

#[derive(Default)]
pub struct Mutes {
    hidden: HashMap<UserId, HashSet<UserId>>,
}

impl Mutes {
    pub fn add(&mut self, who: UserId, whom: UserId) -> bool {
        self.hidden.entry(who).or_default().insert(whom)
    }

    pub fn hidden_by(&self, who: UserId) -> Vec<UserId> {
        let mut hidden: Vec<UserId> = self
            .hidden
            .get(&who)
            .map(|hidden| hidden.iter().copied().collect())
            .unwrap_or_default();
        hidden.sort_unstable_by_key(|user| user.0);
        hidden
    }

    pub fn contains(&self, who: UserId, whom: UserId) -> bool {
        self.hidden
            .get(&who)
            .is_some_and(|hidden| hidden.contains(&whom))
    }
}
