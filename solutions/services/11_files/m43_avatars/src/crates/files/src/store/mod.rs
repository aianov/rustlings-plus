use std::future::Future;
use std::io;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};

use futures::{Stream, StreamExt};
use tokio::io::AsyncWriteExt;

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub type Chunks = Pin<Box<dyn Stream<Item = io::Result<Vec<u8>>> + Send>>;

pub fn chunks_of(data: Vec<u8>, size: usize) -> Chunks {
    let pieces: Vec<io::Result<Vec<u8>>> = data
        .chunks(size.max(1))
        .map(|piece| Ok(piece.to_vec()))
        .collect();
    Box::pin(futures::stream::iter(pieces))
}

pub fn valid_key(key: &str) -> bool {
    (3..=128).contains(&key.len())
        && !key.starts_with('.')
        && key.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"-_.".contains(&byte)
        })
}

fn bad_key(key: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("ключ «{key}» не годится"),
    )
}

pub trait FileStore: Send + Sync {
    fn put(&self, key: &str, body: Chunks) -> BoxFuture<'_, io::Result<u64>>;
    fn get(&self, key: &str) -> BoxFuture<'_, io::Result<Option<Vec<u8>>>>;
    fn exists(&self, key: &str) -> BoxFuture<'_, io::Result<bool>>;
    fn rename(&self, from: &str, to: &str) -> BoxFuture<'_, io::Result<()>>;
    fn remove(&self, key: &str) -> BoxFuture<'_, io::Result<bool>>;
}

pub struct FsFileStore {
    root: PathBuf,
    serial: AtomicU64,
}

struct TempGuard {
    path: PathBuf,
    armed: bool,
}

impl Drop for TempGuard {
    fn drop(&mut self) {
        if self.armed {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

impl FsFileStore {
    pub fn new(root: &Path) -> FsFileStore {
        FsFileStore {
            root: root.to_path_buf(),
            serial: AtomicU64::new(0),
        }
    }

    fn path(&self, key: &str) -> io::Result<PathBuf> {
        if !valid_key(key) {
            return Err(bad_key(key));
        }
        Ok(self.root.join(&key[..2]).join(key))
    }

    async fn write(&self, key: &str, mut body: Chunks) -> io::Result<u64> {
        let target = self.path(key)?;
        let dir = target.parent().map(Path::to_path_buf).unwrap_or_default();
        tokio::fs::create_dir_all(&dir).await?;
        let serial = self.serial.fetch_add(1, Ordering::Relaxed);
        let mut guard = TempGuard {
            path: dir.join(format!(".{key}.{serial}.tmp")),
            armed: true,
        };
        let mut file = tokio::fs::File::create(&guard.path).await?;
        let mut written = 0u64;
        while let Some(piece) = body.next().await {
            let piece = piece?;
            file.write_all(&piece).await?;
            written += piece.len() as u64;
        }
        file.flush().await?;
        file.sync_all().await?;
        drop(file);
        tokio::fs::rename(&guard.path, &target).await?;
        guard.armed = false;
        Ok(written)
    }
}

impl FileStore for FsFileStore {
    fn put(&self, key: &str, body: Chunks) -> BoxFuture<'_, io::Result<u64>> {
        let key = key.to_string();
        Box::pin(async move { self.write(&key, body).await })
    }

    fn get(&self, key: &str) -> BoxFuture<'_, io::Result<Option<Vec<u8>>>> {
        let path = self.path(key);
        Box::pin(async move {
            match tokio::fs::read(path?).await {
                Ok(data) => Ok(Some(data)),
                Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
                Err(error) => Err(error),
            }
        })
    }

    fn exists(&self, key: &str) -> BoxFuture<'_, io::Result<bool>> {
        let path = self.path(key);
        Box::pin(async move { tokio::fs::try_exists(path?).await })
    }

    fn rename(&self, from: &str, to: &str) -> BoxFuture<'_, io::Result<()>> {
        let (from, to) = (self.path(from), self.path(to));
        Box::pin(async move {
            let to = to?;
            if let Some(dir) = to.parent() {
                tokio::fs::create_dir_all(dir).await?;
            }
            tokio::fs::rename(from?, to).await
        })
    }

    fn remove(&self, key: &str) -> BoxFuture<'_, io::Result<bool>> {
        let path = self.path(key);
        Box::pin(async move {
            match tokio::fs::remove_file(path?).await {
                Ok(()) => Ok(true),
                Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
                Err(error) => Err(error),
            }
        })
    }
}
