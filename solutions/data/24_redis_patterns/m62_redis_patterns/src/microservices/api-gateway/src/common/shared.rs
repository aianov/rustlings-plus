use std::collections::HashMap;
use std::net::IpAddr;
use std::time::Duration;

use contracts::{UserId, UserView};
use resp_client::{Client, Script, Value, cmd, key};

use super::guards::RateLimits;

const BUCKET: &str = "
local now = redis.call('TIME')
local at = tonumber(now[1]) * 1000 + math.floor(tonumber(now[2]) / 1000)
local rate = tonumber(ARGV[1])
local burst = tonumber(ARGV[2])
local state = redis.call('HMGET', KEYS[1], 'tokens', 'at')
local tokens = tonumber(state[1]) or burst
local last = tonumber(state[2]) or at
tokens = math.min(burst, tokens + math.max(0, at - last) / 1000 * rate)
local allowed = 0
local wait = 0
if tokens >= 1 then
    tokens = tokens - 1
    allowed = 1
else
    wait = math.ceil((1 - tokens) / rate * 1000)
end
redis.call('HSET', KEYS[1], 'tokens', tokens, 'at', at)
redis.call('PEXPIRE', KEYS[1], math.ceil(burst / rate * 1000) + 1000)
return {allowed, wait}
";

const RELEASE: &str = "
if redis.call('GET', KEYS[1]) == ARGV[1] then
    return redis.call('DEL', KEYS[1])
end
return 0
";

pub const PROFILE_TTL: Duration = Duration::from_secs(300);

pub const PROFILE_LOCK: Duration = Duration::from_secs(2);

pub struct SharedLimits {
    redis: Client,
    limits: RateLimits,
    bucket: Script,
}

impl SharedLimits {
    pub fn new(redis: Client, limits: RateLimits) -> SharedLimits {
        SharedLimits {
            redis,
            limits,
            bucket: Script::new(BUCKET),
        }
    }

    async fn take(&self, kind: &str, id: String, rate: u32, burst: u32) -> Result<(), Duration> {
        let keys = [key("gateway", kind, id)];
        let args = [rate.to_string(), burst.to_string()];
        match self.redis.eval(&self.bucket, &keys, &args).await {
            Ok(value) => {
                let items = value.items();
                let allowed = items.first().and_then(Value::as_int) == Some(1);
                let wait = items.get(1).and_then(Value::as_int).unwrap_or(0);
                match allowed {
                    true => Ok(()),
                    false => Err(Duration::from_millis(u64::try_from(wait).unwrap_or(1))),
                }
            }
            Err(error) => {
                tracing::warn!("api-gateway: общий лимит недоступен, пропускаю: {error}");
                Ok(())
            }
        }
    }

    pub async fn take_user(&self, user: UserId) -> Result<(), Duration> {
        let (rate, burst) = (self.limits.user_rate, self.limits.user_burst);
        self.take("rate-user", user.0.to_string(), rate, burst)
            .await
    }

    pub async fn take_ip(&self, ip: IpAddr) -> Result<(), Duration> {
        let (rate, burst) = (self.limits.ip_rate, self.limits.ip_burst);
        self.take("rate-ip", ip.to_string(), rate, burst).await
    }
}

pub struct RedisPresence {
    redis: Client,
    ttl: Duration,
    instance: String,
    release: Script,
}

fn online_key(user: UserId) -> String {
    key("gateway", "online", user.0)
}

impl RedisPresence {
    pub fn new(redis: Client, ttl: Duration, instance: &str) -> RedisPresence {
        RedisPresence {
            redis,
            ttl,
            instance: instance.to_string(),
            release: Script::new(RELEASE),
        }
    }

    pub async fn touch(&self, user: UserId) {
        let seconds = self.ttl.as_secs().max(1);
        let set = cmd("SET")
            .arg(online_key(user))
            .arg(&self.instance)
            .arg("EX")
            .arg(seconds);
        if let Err(error) = self.redis.query(set).await {
            tracing::warn!("api-gateway: присутствие {user} не записано в Redis: {error}");
        }
    }

    pub async fn release(&self, user: UserId) {
        let keys = [online_key(user)];
        let args = [self.instance.clone()];
        if let Err(error) = self.redis.eval(&self.release, &keys, &args).await {
            tracing::warn!("api-gateway: присутствие {user} не снято, уйдёт по сроку: {error}");
        }
    }

    pub async fn online(&self, user: UserId) -> bool {
        let found = self.redis.query(cmd("EXISTS").arg(online_key(user))).await;
        found.ok().and_then(|value| value.as_int()) == Some(1)
    }

    pub async fn online_many(&self, users: &[UserId]) -> Vec<bool> {
        let commands = users
            .iter()
            .map(|user| cmd("EXISTS").arg(online_key(*user)))
            .collect();
        match self.redis.pipeline(commands).await {
            Ok(replies) => replies
                .iter()
                .map(|reply| reply.as_int() == Some(1))
                .collect(),
            Err(..) => vec![false; users.len()],
        }
    }
}

pub struct RedisProfiles {
    redis: Client,
    ttl: Duration,
}

fn profile_key(user: UserId) -> String {
    key("gateway", "profile", user.0)
}

fn lock_key(user: UserId) -> String {
    key("gateway", "profile-lock", user.0)
}

impl RedisProfiles {
    pub fn new(redis: Client, ttl: Duration) -> RedisProfiles {
        RedisProfiles { redis, ttl }
    }

    fn spread(&self, user: UserId) -> u64 {
        let base = self.ttl.as_secs().max(10);
        base + (user.0.wrapping_mul(2_654_435_761) % (base / 5 + 1))
    }

    pub async fn cached(&self, users: &[UserId]) -> HashMap<UserId, UserView> {
        if users.is_empty() {
            return HashMap::new();
        }
        let keys: Vec<String> = users.iter().map(|user| profile_key(*user)).collect();
        let Ok(found) = self.redis.query(cmd("MGET").args(&keys)).await else {
            return HashMap::new();
        };
        users
            .iter()
            .zip(found.items())
            .filter_map(|(user, raw)| {
                let view = contracts::decode::<UserView>(&raw.as_text()?).ok()?;
                Some((*user, view))
            })
            .collect()
    }

    pub async fn claim(&self, users: &[UserId]) -> Vec<UserId> {
        let millis = PROFILE_LOCK.as_millis().to_string();
        let commands = users
            .iter()
            .map(|user| {
                cmd("SET")
                    .arg(lock_key(*user))
                    .arg(1)
                    .arg("NX")
                    .arg("PX")
                    .arg(&millis)
            })
            .collect();
        let Ok(replies) = self.redis.pipeline(commands).await else {
            return users.to_vec();
        };
        users
            .iter()
            .zip(replies)
            .filter(|(_, reply)| reply.as_text().as_deref() == Some("OK"))
            .map(|(user, _)| *user)
            .collect()
    }

    pub async fn store(&self, views: &[UserView], claimed: &[UserId]) {
        let mut commands: Vec<_> = views
            .iter()
            .filter_map(|view| {
                let raw = contracts::encode(view).ok()?;
                Some(
                    cmd("SET")
                        .arg(profile_key(view.id))
                        .arg(raw)
                        .arg("EX")
                        .arg(self.spread(view.id)),
                )
            })
            .collect();
        commands.extend(claimed.iter().map(|user| cmd("DEL").arg(lock_key(*user))));
        if let Err(error) = self.redis.pipeline(commands).await {
            tracing::warn!("api-gateway: профили не легли в кэш: {error}");
        }
    }

    pub async fn invalidate(&self, user: UserId) {
        if let Err(error) = self.redis.query(cmd("DEL").arg(profile_key(user))).await {
            tracing::warn!("api-gateway: кэш профиля {user} не сброшен: {error}");
        }
    }
}
