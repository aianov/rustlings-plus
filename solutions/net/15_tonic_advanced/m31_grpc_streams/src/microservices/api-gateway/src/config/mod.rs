use std::env;
use std::time::Duration;

use http_engine::ServerConfig;
use proto::{AUTH, CHAT, POSTS, USERS};

pub const HTTP_ADDR_ENV: &str = "GATEWAY_HTTP_ADDR";

pub const DEFAULT_HTTP_ADDR: &str = "127.0.0.1:7880";

pub const CORS_ENV: &str = "GATEWAY_CORS_ORIGINS";

pub const BUDGET: Duration = Duration::from_millis(300);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Services {
    pub auth: String,
    pub users: String,
    pub posts: String,
    pub chat: String,
}

impl Default for Services {
    fn default() -> Services {
        Services {
            auth: AUTH.default.to_string(),
            users: USERS.default.to_string(),
            posts: POSTS.default.to_string(),
            chat: CHAT.default.to_string(),
        }
    }
}

impl Services {
    pub fn from_env() -> Services {
        Services {
            auth: AUTH.addr(),
            users: USERS.addr(),
            posts: POSTS.addr(),
            chat: CHAT.addr(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub max_connections: usize,
    pub idle_timeout: Duration,
    pub ping_every: Duration,
    pub max_in_flight: usize,
    pub max_line: usize,
    pub outbox: usize,
    pub services: Services,
    pub budget: Duration,
    pub http: ServerConfig,
    pub access_log: bool,
    pub cors_origins: Vec<String>,
    pub avatar_max: usize,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            max_connections: 256,
            idle_timeout: Duration::from_secs(90),
            ping_every: Duration::from_secs(30),
            max_in_flight: 8,
            max_line: 64 * 1024,
            outbox: 64,
            services: Services::default(),
            budget: BUDGET,
            http: ServerConfig::default(),
            access_log: false,
            cors_origins: Vec::new(),
            avatar_max: 256 * 1024,
        }
    }
}

impl Config {
    pub fn from_env() -> Config {
        Config {
            services: Services::from_env(),
            cors_origins: env::var(CORS_ENV)
                .map(|list| {
                    list.split(',')
                        .map(str::trim)
                        .filter(|origin| !origin.is_empty())
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default(),
            ..Config::default()
        }
    }
}
