use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use social_core::{MemPostStore, Post, PostId, PostStore};

use crate::ids::SeqIds;

#[derive(Debug, Clone, Default)]
pub struct Scans {
    count: Arc<AtomicUsize>,
}

impl Scans {
    pub fn get(&self) -> usize {
        self.count.load(Ordering::Relaxed)
    }

    fn bump(&self) {
        self.count.fetch_add(1, Ordering::Relaxed);
    }
}

pub struct FakePosts {
    inner: MemPostStore,
    ids: SeqIds,
    scans: Scans,
}

impl FakePosts {
    pub fn starting_at(first: u64) -> FakePosts {
        FakePosts {
            inner: MemPostStore::new(),
            ids: SeqIds::starting_at(first),
            scans: Scans::default(),
        }
    }

    pub fn scans(&self) -> Scans {
        self.scans.clone()
    }
}

impl PostStore for FakePosts {
    fn next_id(&mut self) -> PostId {
        PostId(self.ids.next_id())
    }

    fn insert(&mut self, post: Post) {
        self.inner.insert(post);
    }

    fn get(&self, id: PostId) -> Option<&Post> {
        self.inner.get(id)
    }

    fn get_mut(&mut self, id: PostId) -> Option<&mut Post> {
        self.inner.get_mut(id)
    }

    fn all(&self) -> &[Post] {
        self.scans.bump();
        self.inner.all()
    }

    fn count(&self) -> usize {
        self.inner.count()
    }
}
