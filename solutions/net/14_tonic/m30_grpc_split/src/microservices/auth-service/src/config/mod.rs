use std::env;
use std::path::PathBuf;

use http_engine::ServerConfig;
use proto::USERS;

pub const HTTP_ADDR_ENV: &str = "AUTH_HTTP_ADDR";

pub const DEFAULT_HTTP_ADDR: &str = "127.0.0.1:7881";

pub const JOURNAL_ENV: &str = "AUTH_JOURNAL";

pub const CORS_ENV: &str = "AUTH_CORS_ORIGINS";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub users: String,
    pub journal: Option<PathBuf>,
    pub http: ServerConfig,
    pub access_log: bool,
    pub cors_origins: Vec<String>,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            users: USERS.default.to_string(),
            journal: None,
            http: ServerConfig::default(),
            access_log: false,
            cors_origins: Vec::new(),
        }
    }
}

impl Config {
    pub fn from_env() -> Config {
        Config {
            users: USERS.addr(),
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
