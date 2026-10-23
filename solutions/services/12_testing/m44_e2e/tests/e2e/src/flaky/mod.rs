use std::io;
use std::sync::Arc;

use files::{BoxFuture, Chunks, FileStore};
use test_support::Faults;

pub struct FlakyFiles {
    inner: Arc<dyn FileStore>,
    puts: Faults,
    reads: Faults,
}

fn refused(what: &str) -> io::Error {
    io::Error::other(format!("{what}: отказ, подстроенный тестом"))
}

impl FlakyFiles {
    pub fn new(inner: Arc<dyn FileStore>, puts: Faults, reads: Faults) -> FlakyFiles {
        FlakyFiles { inner, puts, reads }
    }
}

impl FileStore for FlakyFiles {
    fn put(&self, key: &str, body: Chunks) -> BoxFuture<'_, io::Result<u64>> {
        if self.puts.hit() {
            return Box::pin(async { Err(refused("put")) });
        }
        self.inner.put(key, body)
    }

    fn get(&self, key: &str) -> BoxFuture<'_, io::Result<Option<Vec<u8>>>> {
        if self.reads.hit() {
            return Box::pin(async { Err(refused("get")) });
        }
        self.inner.get(key)
    }

    fn exists(&self, key: &str) -> BoxFuture<'_, io::Result<bool>> {
        self.inner.exists(key)
    }

    fn rename(&self, from: &str, to: &str) -> BoxFuture<'_, io::Result<()>> {
        if self.puts.hit() {
            return Box::pin(async { Err(refused("rename")) });
        }
        self.inner.rename(from, to)
    }

    fn remove(&self, key: &str) -> BoxFuture<'_, io::Result<bool>> {
        self.inner.remove(key)
    }
}
