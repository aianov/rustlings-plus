use bytes::Bytes;
use files::content::put_content;
use files::image::{AvatarError as ImageError, MAX_BYTES, check_avatar};
use files::limit::limit;
use files::sign::{LinkError, Opened, Signer};
use files::{Body, FileStore};
use futures::StreamExt;
use std::collections::HashMap;
use std::io;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

pub const GRACE: u64 = 600;
pub const LINK_TTL: u64 = 3600;

#[derive(Debug, PartialEq)]
pub enum AvatarError {
    TooLarge,
    NotImage,
    BadSize,
    Unsupported,
    Storage,
    Db,
}

#[derive(Default)]
struct Rows {
    avatars: HashMap<u64, String>,
    pending: HashMap<String, u64>,
    temps: u64,
}

#[derive(Default)]
pub struct Db {
    rows: Mutex<Rows>,
    pub fail_commit: AtomicBool,
}

impl Db {
    pub fn next_temp(&self) -> String {
        let mut rows = self.rows.lock().unwrap();
        rows.temps += 1;
        format!("tmp-{}", rows.temps)
    }

    pub fn begin_upload(&self, key: &str, now: u64) {
        self.rows.lock().unwrap().pending.insert(key.to_string(), now);
    }

    pub fn forget_pending(&self, key: &str) {
        self.rows.lock().unwrap().pending.remove(key);
    }

    pub fn stale_pending(&self, cutoff: u64) -> Vec<String> {
        let mut keys: Vec<String> = self.rows.lock().unwrap().pending.iter().filter(|(_, started)| **started <= cutoff).map(|(k, _)| k.clone()).collect();
        keys.sort();
        keys
    }

    pub fn commit_avatar(&self, user: u64, key: &str) -> Result<Option<String>, AvatarError> {
        if self.fail_commit.load(Ordering::SeqCst) {
            return Err(AvatarError::Db);
        }
        let mut rows = self.rows.lock().unwrap();
        rows.pending.remove(key);
        Ok(rows.avatars.insert(user, key.to_string()))
    }

    pub fn avatar_of(&self, user: u64) -> Option<String> {
        self.rows.lock().unwrap().avatars.get(&user).cloned()
    }

    pub fn refs_to(&self, key: &str) -> usize {
        self.rows.lock().unwrap().avatars.values().filter(|k| *k == key).count()
    }

    pub fn pending_count(&self) -> usize {
        self.rows.lock().unwrap().pending.len()
    }
}

pub struct AvatarService<S> {
    pub store: S,
    pub db: Db,
    signer: Signer,
}

impl<S: FileStore> AvatarService<S> {
    pub fn new(store: S, secret: &[u8]) -> AvatarService<S> {
        AvatarService { store, db: Db::default(), signer: Signer::new(secret) }
    }

    pub async fn set_avatar(&self, user: u64, body: Body, now: u64) -> Result<String, AvatarError> {
        let mut limited = limit(body, MAX_BYTES as u64, None).map_err(|_| AvatarError::TooLarge)?;
        let mut bytes = Vec::new();
        while let Some(chunk) = limited.next().await {
            match chunk {
                Ok(chunk) => bytes.extend_from_slice(&chunk),
                Err(e) if e.kind() == io::ErrorKind::FileTooLarge => return Err(AvatarError::TooLarge),
                Err(_) => return Err(AvatarError::Storage),
            }
        }
        check_avatar(&bytes).map_err(|e| match e {
            ImageError::NotImage | ImageError::Truncated => AvatarError::NotImage,
            ImageError::TooBig => AvatarError::TooLarge,
            ImageError::BadSize { .. } => AvatarError::BadSize,
            ImageError::Unsupported => AvatarError::Unsupported,
        })?;
        let temp = self.db.next_temp();
        self.db.begin_upload(&temp, now);
        let whole: Body = futures::stream::iter(vec![Ok(Bytes::from(bytes))]).boxed();
        let stored = match put_content(&self.store, whole, &temp).await {
            Ok(stored) => stored,
            Err(_) => {
                self.db.forget_pending(&temp);
                return Err(AvatarError::Storage);
            }
        };
        self.db.begin_upload(&stored.key, now);
        self.db.forget_pending(&temp);
        let previous = self.db.commit_avatar(user, &stored.key)?;
        if let Some(old) = previous.filter(|old| *old != stored.key) {
            if self.db.refs_to(&old) == 0 {
                let _ = self.store.delete(&old).await;
            }
        }
        Ok(stored.key)
    }

    pub async fn sweep(&self, now: u64) -> usize {
        let mut removed = 0;
        for key in self.db.stale_pending(now.saturating_sub(GRACE)) {
            if self.db.refs_to(&key) == 0 && self.store.delete(&key).await.unwrap_or(false) {
                removed += 1;
            }
            self.db.forget_pending(&key);
        }
        removed
    }

    pub fn avatar_link(&self, user: u64, now: u64) -> Option<String> {
        let key = self.db.avatar_of(user)?;
        Some(self.signer.link(&key, now + LINK_TTL, "avatar"))
    }

    pub fn open(&self, path_and_query: &str, now: u64) -> Result<Opened, LinkError> {
        self.signer.verify(path_and_query, now)
    }

    pub async fn read(&self, key: &str) -> io::Result<Option<Vec<u8>>> {
        let Some(mut body) = self.store.get(key).await? else {
            return Ok(None);
        };
        let mut out = Vec::new();
        while let Some(chunk) = body.next().await {
            out.extend_from_slice(&chunk?);
        }
        Ok(Some(out))
    }
}
