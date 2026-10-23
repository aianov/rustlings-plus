use std::env;
use std::path::PathBuf;
use std::time::Duration;

pub const ADDR_ENV: &str = "GATEWAY_ADDR";

pub const DEFAULT_ADDR: &str = "127.0.0.1:7878";

pub const JOURNAL_ENV: &str = "SOCIAL_JOURNAL";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub max_connections: usize,
    pub idle_timeout: Duration,
    pub max_line: usize,
    pub accept_pause: Duration,
    pub outbox: usize,
    pub drain: Duration,
    pub journal: Option<PathBuf>,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            max_connections: 256,
            idle_timeout: Duration::from_secs(300),
            max_line: 64 * 1024,
            accept_pause: Duration::from_millis(100),
            outbox: 64,
            drain: Duration::from_secs(5),
            journal: None,
        }
    }
}

impl Config {
    pub fn from_env() -> Config {
        Config {
            journal: env::var_os(JOURNAL_ENV).map(PathBuf::from),
            ..Config::default()
        }
    }
}
