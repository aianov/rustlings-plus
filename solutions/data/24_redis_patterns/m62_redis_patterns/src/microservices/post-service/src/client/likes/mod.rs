use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use contracts::PostId;
use resp_client::{Client, RespError, cmd, key, scan_keys};
use service_runtime::CancellationToken;

use crate::repository::{BoxFuture, PendingLikes, PgPosts};

pub const LIKE_FLUSH_EVERY: Duration = Duration::from_secs(2);

pub struct RedisLikes {
    redis: Client,
    instance: String,
    round: AtomicU64,
}

pub fn pending_key() -> String {
    key("posts", "likes-pending", "all")
}

fn flushing_key(instance: &str, round: u64) -> String {
    key("posts", "likes-flushing", format!("{instance}-{round}"))
}

impl RedisLikes {
    pub fn new(redis: Client, instance: &str) -> RedisLikes {
        RedisLikes {
            redis,
            instance: instance.to_string(),
            round: AtomicU64::new(0),
        }
    }

    fn next_key(&self) -> String {
        flushing_key(&self.instance, self.round.fetch_add(1, Ordering::Relaxed))
    }

    async fn claim(&self, from: &str) -> Result<Option<String>, RespError> {
        let to = self.next_key();
        match self.redis.query(cmd("RENAME").arg(from).arg(&to)).await {
            Ok(..) => Ok(Some(to)),
            Err(RespError::Server(why)) if why.contains("no such key") => Ok(None),
            Err(error) => Err(error),
        }
    }

    async fn apply(&self, posts: &PgPosts, working: &str) -> Result<u64, RespError> {
        let raw = self.redis.query(cmd("HGETALL").arg(working)).await?.items();
        let deltas: Vec<(PostId, i64)> = raw
            .chunks(2)
            .filter_map(|pair| match pair {
                [post, delta] => Some((PostId(post.as_text()?.parse().ok()?), delta.as_int()?)),
                _ => None,
            })
            .collect();
        if !deltas.is_empty() {
            if let Err(error) = posts.apply_likes(&deltas).await {
                tracing::warn!("post-service: лайки не сброшены в Postgres, повторю: {error}");
                return Ok(0);
            }
        }
        self.redis.del(&[working.to_string()]).await?;
        Ok(deltas
            .iter()
            .map(|(_, delta)| u64::try_from(*delta).unwrap_or(0))
            .sum())
    }

    pub async fn flush(&self, posts: &PgPosts) -> Result<u64, RespError> {
        let mut moved = 0;
        let pattern = key("posts", "likes-flushing", "*");
        for left in scan_keys(&self.redis, &pattern, 100).await? {
            if let Some(working) = self.claim(&left).await? {
                moved += self.apply(posts, &working).await?;
            }
        }
        if let Some(working) = self.claim(&pending_key()).await? {
            moved += self.apply(posts, &working).await?;
        }
        Ok(moved)
    }
}

impl PendingLikes for RedisLikes {
    fn add(&self, post: PostId) -> BoxFuture<'_, bool> {
        Box::pin(async move {
            let added = self
                .redis
                .query(cmd("HINCRBY").arg(pending_key()).arg(post.0).arg(1))
                .await;
            added.is_ok()
        })
    }
}

pub async fn flush_likes(
    likes: Arc<RedisLikes>,
    posts: Arc<PgPosts>,
    every: Duration,
    closing: CancellationToken,
) {
    loop {
        tokio::select! {
            () = closing.cancelled() => {
                let _ = likes.flush(&posts).await;
                return;
            }
            () = tokio::time::sleep(every) => {}
        }
        if let Err(error) = likes.flush(&posts).await {
            tracing::warn!("post-service: сброс лайков не прошёл: {error}");
        }
    }
}
