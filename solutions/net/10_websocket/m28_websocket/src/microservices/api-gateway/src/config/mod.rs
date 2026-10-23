use std::env;
use std::path::PathBuf;
use std::time::Duration;

use http_engine::ServerConfig;

pub const HTTP_ADDR_ENV: &str = "GATEWAY_HTTP_ADDR";

pub const DEFAULT_HTTP_ADDR: &str = "127.0.0.1:7880";

pub const JOURNAL_ENV: &str = "SOCIAL_JOURNAL";

pub const CORS_ENV: &str = "GATEWAY_CORS_ORIGINS";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub max_connections: usize,
    pub idle_timeout: Duration,
    pub max_line: usize,
    pub outbox: usize,
    pub drain: Duration,
    pub journal: Option<PathBuf>,
    pub http: ServerConfig,
    pub access_log: bool,
    pub cors_origins: Vec<String>,
    pub avatar_max: usize,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            max_connections: 256,
            idle_timeout: Duration::from_secs(300),
            max_line: 64 * 1024,
            outbox: 64,
            drain: Duration::from_secs(5),
            journal: None,
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
            journal: env::var_os(JOURNAL_ENV).map(PathBuf::from),
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
