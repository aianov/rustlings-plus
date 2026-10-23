use crate::{Body, CHUNK, FileStore};
use bytes::Bytes;
use futures::StreamExt;
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub struct FsStore {
    root: PathBuf,
}

impl FsStore {
    pub fn new(root: PathBuf) -> FsStore {
        FsStore { root }
    }

    fn path_of(&self, key: &str) -> PathBuf {
        self.root.join(key.get(..2).unwrap_or("__")).join(key)
    }
}

impl FileStore for FsStore {
    async fn put(&self, key: &str, body: Body) -> io::Result<u64> {
        let _ = (key, body, self.path_of(key), AtomicU64::new(0).load(Ordering::SeqCst));
        todo!()
    }

    async fn get(&self, key: &str) -> io::Result<Option<Body>> {
        let file = match tokio::fs::File::open(self.path_of(key)).await {
            Ok(file) => file,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e),
        };
        let body = futures::stream::unfold(file, |mut file| async move {
            let mut buf = vec![0u8; CHUNK];
            match file.read(&mut buf).await {
                Ok(0) => None,
                Ok(n) => {
                    buf.truncate(n);
                    Some((Ok(Bytes::from(buf)), file))
                }
                Err(e) => Some((Err(e), file)),
            }
        });
        Ok(Some(body.boxed()))
    }

    async fn exists(&self, key: &str) -> io::Result<bool> {
        match tokio::fs::metadata(self.path_of(key)).await {
            Ok(meta) => Ok(meta.is_file()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e),
        }
    }

    async fn rename(&self, from: &str, to: &str) -> io::Result<()> {
        let target = self.path_of(to);
        tokio::fs::create_dir_all(target.parent().expect("у пути всегда есть каталог")).await?;
        tokio::fs::rename(self.path_of(from), target).await
    }

    async fn delete(&self, key: &str) -> io::Result<bool> {
        match tokio::fs::remove_file(self.path_of(key)).await {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e),
        }
    }
}
