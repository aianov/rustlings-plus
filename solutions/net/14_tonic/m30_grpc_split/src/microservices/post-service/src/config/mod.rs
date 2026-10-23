use std::env;
use std::path::PathBuf;
use std::time::Duration;

use proto::USERS;

pub const JOURNAL_ENV: &str = "POSTS_JOURNAL";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub users: String,
    pub journal: Option<PathBuf>,
    pub drain: Duration,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            users: USERS.default.to_string(),
            journal: None,
            drain: Duration::from_secs(5),
        }
    }
}

impl Config {
    pub fn from_env() -> Config {
        Config {
            users: USERS.addr(),
            journal: env::var_os(JOURNAL_ENV).map(PathBuf::from),
            ..Config::default()
        }
    }
}
