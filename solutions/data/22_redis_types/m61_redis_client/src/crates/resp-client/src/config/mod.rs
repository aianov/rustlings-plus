use std::time::Duration;

pub const REDIS_URL_ENV: &str = "RUSTLINGS_REDIS_URL";

pub const DEFAULT_REDIS_ADDR: &str = "127.0.0.1:56379";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedisConfig {
    pub addr: String,
    pub timeout: Duration,
    pub connect_timeout: Duration,
}

impl RedisConfig {
    pub fn new(addr: &str) -> RedisConfig {
        let addr = addr.trim();
        let addr = addr.strip_prefix("redis://").unwrap_or(addr);
        let addr = addr.trim_end_matches('/');
        RedisConfig {
            addr: addr.to_string(),
            timeout: Duration::from_millis(500),
            connect_timeout: Duration::from_secs(2),
        }
    }

    pub fn from_env() -> RedisConfig {
        let addr = std::env::var(REDIS_URL_ENV)
            .ok()
            .filter(|addr| !addr.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_REDIS_ADDR.to_string());
        RedisConfig::new(&addr)
    }

    pub fn timeout(self, timeout: Duration) -> RedisConfig {
        RedisConfig { timeout, ..self }
    }
}
