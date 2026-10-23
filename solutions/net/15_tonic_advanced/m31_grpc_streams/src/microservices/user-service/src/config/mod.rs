use std::env;
use std::path::PathBuf;
use std::time::Duration;

use grpc_kit::LogSink;

pub const JOURNAL_ENV: &str = "USERS_JOURNAL";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub journal: Option<PathBuf>,
    pub drain: Duration,
    pub grace: Duration,
    pub log: Option<LogSink>,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            journal: None,
            drain: Duration::from_secs(5),
            grace: Duration::from_millis(100),
            log: None,
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
