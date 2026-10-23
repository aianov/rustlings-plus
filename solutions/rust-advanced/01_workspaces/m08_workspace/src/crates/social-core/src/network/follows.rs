use std::collections::HashMap;

use crate::domain::UserId;

#[derive(Default)]
pub struct Follows {
    following: HashMap<UserId, Vec<UserId>>,
    followers: HashMap<UserId, Vec<UserId>>,
}

impl Follows {
    pub fn add(&mut self, who: UserId, whom: UserId) -> bool {
        let list = self.following.entry(who).or_default();
        if list.contains(&whom) {
            return false;
        }
        list.push(whom);
        self.followers.entry(whom).or_default().push(who);
        true
    }

    pub fn contains(&self, who: UserId, whom: UserId) -> bool {
        match self.following.get(&who) {
            Some(list) => list.contains(&whom),
            None => false,
        }
    }

    pub fn following_count(&self, who: UserId) -> usize {
        count(&self.following, who)
    }

    pub fn followers_count(&self, whom: UserId) -> usize {
        count(&self.followers, whom)
    }
}

fn count(lists: &HashMap<UserId, Vec<UserId>>, user: UserId) -> usize {
    match lists.get(&user) {
        Some(list) => list.len(),
        None => 0,
    }
}
