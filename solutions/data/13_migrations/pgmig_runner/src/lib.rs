mod files;
mod runner;

pub use files::parse_dir;
pub use runner::Migrator;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Migration {
    pub version: i64,
    pub name: String,
    pub sql: String,
}

#[derive(Debug, Clone)]
pub struct Options {
    pub lock_key: i64,
    pub lock_timeout_ms: u64,
    pub attempts: u32,
    pub retry_pause_ms: u64,
}

impl Default for Options {
    fn default() -> Options {
        Options { lock_key: 7_575_757, lock_timeout_ms: 300, attempts: 10, retry_pause_ms: 100 }
    }
}

#[derive(Debug)]
pub enum MigrateError {
    BadFileName(String),
    DuplicateVersion(i64),
    Drift { version: i64 },
    MissingFile { version: i64 },
    OutOfOrder { version: i64 },
    LockTimeout { version: i64 },
    Failed { version: i64, error: sqlx::Error },
    Db(sqlx::Error),
    Io(std::io::Error),
}

impl From<sqlx::Error> for MigrateError {
    fn from(e: sqlx::Error) -> MigrateError {
        MigrateError::Db(e)
    }
}

impl From<std::io::Error> for MigrateError {
    fn from(e: std::io::Error) -> MigrateError {
        MigrateError::Io(e)
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub applied: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    Applied,
    Pending,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    pub version: i64,
    pub name: String,
    pub state: State,
}
