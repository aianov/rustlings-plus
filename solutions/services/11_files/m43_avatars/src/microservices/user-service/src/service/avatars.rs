use std::time::Duration;

use contracts::UserId;
use files::{Chunks, HEAD_MAX, check_avatar, limit, put_content};
use futures::StreamExt;
use proto::AVATAR_MAX;

use super::{UserError, UserService};
use crate::common::{Avatar, now_micros};

pub const AVATAR_CHUNK: usize = 16 * 1024;

pub const ORPHAN_GRACE: Duration = Duration::from_secs(60 * 60);

fn broken(error: std::io::Error) -> UserError {
    match error
        .get_ref()
        .and_then(|inner| inner.downcast_ref::<files::FileError>())
    {
        Some(..) => UserError::TooLarge(AVATAR_MAX),
        None => UserError::Files(error.to_string()),
    }
}

impl UserService {
    fn store(&self) -> Result<&dyn files::FileStore, UserError> {
        self.files.as_deref().ok_or(UserError::NoFiles)
    }

    pub async fn put_avatar(
        &self,
        owner: UserId,
        body: Chunks,
        declared: Option<u64>,
    ) -> Result<(Avatar, bool), UserError> {
        self.profile(owner).await?;
        let store = self.store()?;
        let mut body = limit(body, AVATAR_MAX as u64, declared)
            .map_err(|_| UserError::TooLarge(AVATAR_MAX))?;
        let mut head = Vec::new();
        let picture = loop {
            match check_avatar(&head) {
                Ok(picture) => break picture,
                Err(files::ImageError::Truncated) if head.len() < HEAD_MAX => {}
                Err(error) => return Err(UserError::NotImage(error.to_string())),
            }
            match body.next().await {
                Some(piece) => head.extend(piece.map_err(broken)?),
                None => {
                    let error = check_avatar(&head)
                        .err()
                        .unwrap_or(files::ImageError::Truncated);
                    return Err(UserError::NotImage(error.to_string()));
                }
            }
        };
        let rest: Chunks = Box::pin(futures::stream::iter([Ok(head)]).chain(body));
        let temp = format!("tmp-{}-{}", owner.0, now_micros());
        let at = now_micros() / 1_000_000;
        self.avatars.pending(temp.clone(), at).await;
        let stored = put_content(store, rest, &temp).await.map_err(broken)?;
        self.avatars.pending(stored.key.clone(), at).await;
        self.avatars.settle(temp).await;
        let avatar = Avatar {
            key: stored.key.clone(),
            content_type: picture.kind.mime().to_string(),
        };
        let previous = self.avatars.link(owner, avatar.clone()).await;
        self.avatars.settle(stored.key).await;
        if let Some(previous) = &previous {
            self.forget_file(&previous.key).await;
        }
        Ok((avatar, previous.is_none()))
    }

    async fn forget_file(&self, key: &str) {
        if self.avatars.referenced(key.to_string()).await {
            return;
        }
        if let Ok(store) = self.store() {
            let _ = store.remove(key).await;
        }
    }

    pub async fn avatar(&self, owner: UserId) -> Result<(Avatar, Vec<u8>), UserError> {
        let avatar = self
            .avatars
            .get(owner)
            .await
            .ok_or(UserError::NoAvatar(owner))?;
        let data = self.file(&avatar.key).await?;
        Ok((avatar, data))
    }

    pub async fn avatar_of(&self, owner: UserId) -> Option<Avatar> {
        self.avatars.get(owner).await
    }

    pub async fn file(&self, key: &str) -> Result<Vec<u8>, UserError> {
        let found = self.store()?.get(key).await;
        found
            .map_err(|error| UserError::Files(error.to_string()))?
            .ok_or_else(|| UserError::NoFile(key.to_string()))
    }

    pub async fn delete_avatar(&self, owner: UserId) -> bool {
        match self.avatars.remove(owner).await {
            Some(previous) => {
                self.forget_file(&previous.key).await;
                true
            }
            None => false,
        }
    }

    pub async fn sweep(&self, now_secs: u64, grace: Duration) -> usize {
        let before = now_secs.saturating_sub(grace.as_secs());
        let mut removed = 0;
        for key in self.avatars.stale(before).await {
            if !self.avatars.referenced(key.clone()).await {
                if let Ok(store) = self.store() {
                    if store.remove(&key).await.unwrap_or(false) {
                        removed += 1;
                    }
                }
            }
            self.avatars.settle(key).await;
        }
        removed
    }
}
