use std::env;
use std::path::PathBuf;
use std::time::Duration;

pub const JOURNAL_ENV: &str = "USERS_JOURNAL";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub journal: Option<PathBuf>,
    pub drain: Duration,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            journal: None,
            drain: Duration::from_secs(5),
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
