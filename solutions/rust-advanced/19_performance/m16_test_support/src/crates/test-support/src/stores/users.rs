use social_core::{MemUserStore, User, UserId, UserStore};

use crate::ids::SeqIds;

pub struct FakeUsers {
    inner: MemUserStore,
    ids: SeqIds,
}

impl FakeUsers {
    pub fn starting_at(first: u64) -> FakeUsers {
        FakeUsers {
            inner: MemUserStore::new(),
            ids: SeqIds::starting_at(first),
        }
    }
}

impl UserStore for FakeUsers {
    fn next_id(&mut self) -> UserId {
        UserId(self.ids.next_id())
    }

    fn insert(&mut self, user: User) {
        self.inner.insert(user);
    }

    fn get(&self, id: UserId) -> Option<&User> {
        self.inner.get(id)
    }

    fn id_by_name(&self, name: &str) -> Option<UserId> {
        self.inner.id_by_name(name)
    }

    fn count(&self) -> usize {
        self.inner.count()
    }
}
