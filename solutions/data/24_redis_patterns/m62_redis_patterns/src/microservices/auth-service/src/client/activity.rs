use std::time::Duration;

use resp_client::{Client, RespError, cmd, key};

use crate::service::{Activity, BoxActivity, ThrottleKey};

pub struct RedisActivity {
    redis: Client,
    forget: Duration,
    idle: Duration,
}

fn fails_key(key_of: &ThrottleKey) -> String {
    match key_of {
        ThrottleKey::Account(name) => key("auth", "fails-account", name),
        ThrottleKey::Address(ip) => key("auth", "fails-ip", ip),
    }
}

pub fn active_key(session: &str) -> String {
    key("auth", "active", session)
}

pub fn sessions_key(user: u64) -> String {
    key("auth", "sessions", user)
}

fn logged<T: Default>(what: &str, done: Result<T, RespError>) -> T {
    match done {
        Ok(value) => value,
        Err(error) => {
            tracing::warn!("auth-service: Redis не ответил на {what}: {error}");
            T::default()
        }
    }
}

impl RedisActivity {
    pub fn new(redis: Client, forget: Duration, idle: Duration) -> RedisActivity {
        RedisActivity {
            redis,
            forget,
            idle,
        }
    }
}

impl Activity for RedisActivity {
    fn strikes(&self, keys: Vec<ThrottleKey>) -> BoxActivity<'_, u32> {
        Box::pin(async move {
            let commands = keys
                .iter()
                .map(|one| cmd("GET").arg(fails_key(one)))
                .collect();
            let replies = logged("счётчики входа", self.redis.pipeline(commands).await);
            replies
                .iter()
                .filter_map(|reply| reply.as_int())
                .filter_map(|count| u32::try_from(count).ok())
                .max()
                .unwrap_or(0)
        })
    }

    fn failed(&self, keys: Vec<ThrottleKey>) -> BoxActivity<'_, ()> {
        Box::pin(async move {
            let forget = self.forget.as_secs().max(1);
            let commands = keys
                .iter()
                .flat_map(|one| {
                    let key = fails_key(one);
                    [cmd("INCR").arg(&key), cmd("EXPIRE").arg(&key).arg(forget)]
                })
                .collect();
            logged("неудачный вход", self.redis.pipeline(commands).await);
        })
    }

    fn cleared(&self, account: String) -> BoxActivity<'_, ()> {
        Box::pin(async move {
            let key = fails_key(&ThrottleKey::Account(account));
            logged("сброс счётчика", self.redis.del(&[key]).await);
        })
    }

    fn opened(&self, user: u64, session: String) -> BoxActivity<'_, ()> {
        Box::pin(async move {
            let idle = self.idle.as_secs().max(1);
            let sessions = sessions_key(user);
            let commands = vec![
                cmd("SET")
                    .arg(active_key(&session))
                    .arg(user)
                    .arg("EX")
                    .arg(idle),
                cmd("SADD").arg(&sessions).arg(&session),
                cmd("EXPIRE").arg(&sessions).arg(30 * 24 * 60 * 60),
            ];
            logged("новая сессия", self.redis.pipeline(commands).await);
        })
    }

    fn touched(&self, session: String) -> BoxActivity<'_, bool> {
        Box::pin(async move {
            let idle = self.idle.as_secs().max(1);
            match self.redis.expire(&active_key(&session), idle).await {
                Ok(alive) => alive,
                Err(error) => {
                    tracing::warn!("auth-service: активность сессии не проверена: {error}");
                    true
                }
            }
        })
    }

    fn closed_all(&self, user: u64) -> BoxActivity<'_, usize> {
        Box::pin(async move {
            let sessions = sessions_key(user);
            let members = logged(
                "сессии",
                self.redis.query(cmd("SMEMBERS").arg(&sessions)).await,
            );
            let mut keys: Vec<String> = members
                .items()
                .iter()
                .filter_map(|member| member.as_text())
                .map(|session| active_key(&session))
                .collect();
            let closed = keys.len();
            keys.push(sessions);
            logged("выход отовсюду", self.redis.del(&keys).await);
            closed
        })
    }
}
