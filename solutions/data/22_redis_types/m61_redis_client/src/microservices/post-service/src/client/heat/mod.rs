use contracts::{PostId, UserId};
use resp_client::{Client, RespError, cmd, key};

use crate::service::{BoxHeat, Heat};

pub const VIEWS_TTL: u64 = 30 * 24 * 60 * 60;

pub const TAGS_TTL: u64 = 48 * 60 * 60;

pub const TAG_HOURS: u64 = 24;

const HOUR_MICROS: u64 = 60 * 60 * 1_000_000;

pub struct RedisHeat {
    redis: Client,
}

fn quiet<T: Default>(done: Result<T, RespError>) -> T {
    match done {
        Ok(value) => value,
        Err(error) => {
            tracing::warn!("post-service: Redis не ответил: {error}");
            T::default()
        }
    }
}

pub fn views_key(post: PostId) -> String {
    key("posts", "views", post.0)
}

pub fn tags_key(hour: u64) -> String {
    key("posts", "tags", hour)
}

impl RedisHeat {
    pub fn new(redis: Client) -> RedisHeat {
        RedisHeat { redis }
    }
}

impl Heat for RedisHeat {
    fn view(&self, post: PostId, viewer: UserId) -> BoxHeat<'_, u64> {
        Box::pin(async move {
            let key = views_key(post);
            let replies = self
                .redis
                .pipeline(vec![
                    cmd("PFADD").arg(&key).arg(viewer.0),
                    cmd("EXPIRE").arg(&key).arg(VIEWS_TTL),
                    cmd("PFCOUNT").arg(&key),
                ])
                .await;
            let replies = quiet(replies);
            replies
                .last()
                .and_then(|count| count.as_int())
                .and_then(|count| u64::try_from(count).ok())
                .unwrap_or(0)
        })
    }

    fn views(&self, posts: Vec<PostId>) -> BoxHeat<'_, Vec<u64>> {
        Box::pin(async move {
            let commands = posts
                .iter()
                .map(|post| cmd("PFCOUNT").arg(views_key(*post)))
                .collect();
            let replies = quiet(self.redis.pipeline(commands).await);
            let mut counts: Vec<u64> = replies
                .iter()
                .map(|count| {
                    count
                        .as_int()
                        .and_then(|count| u64::try_from(count).ok())
                        .unwrap_or(0)
                })
                .collect();
            counts.resize(posts.len(), 0);
            counts
        })
    }

    fn tagged(&self, tags: Vec<String>, at: u64) -> BoxHeat<'_, ()> {
        Box::pin(async move {
            let key = tags_key(at / HOUR_MICROS);
            let mut commands: Vec<_> = tags
                .iter()
                .map(|tag| cmd("ZINCRBY").arg(&key).arg(1).arg(tag))
                .collect();
            commands.push(cmd("EXPIRE").arg(&key).arg(TAGS_TTL));
            quiet(self.redis.pipeline(commands).await);
        })
    }

    fn top_tags(&self, limit: usize, now: u64) -> BoxHeat<'_, Vec<(String, u64)>> {
        Box::pin(async move {
            let hour = now / HOUR_MICROS;
            let hours: Vec<String> = (0..TAG_HOURS)
                .map(|back| tags_key(hour.saturating_sub(back)))
                .collect();
            let dest = key("posts", "tags-top", hour);
            let built = self
                .redis
                .pipeline(vec![
                    cmd("ZUNIONSTORE").arg(&dest).arg(hours.len()).args(&hours),
                    cmd("EXPIRE").arg(&dest).arg(60),
                ])
                .await;
            quiet(built);
            let top = quiet(self.redis.zrevrange_scores(&dest, limit).await);
            top.into_iter()
                .map(|(tag, score)| (tag, score.max(0.0) as u64))
                .collect()
        })
    }
}
