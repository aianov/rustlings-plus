use std::path::PathBuf;

use grpc_kit::{ACCESS_LOG_ENV, CERTS_ENV, DEFAULT_CERTS, LogSink, Tls};
use proto::{CHAT, USERS};
use service_runtime::{CancellationToken, ConfigError, Health, Lifecycle, Source, Timing, Trail};

pub const NAME: &str = "chat-service";

pub const JOURNAL_ENV: &str = "CHAT_JOURNAL";

pub const ADMIN_ENV: &str = "CHAT_ADMIN_ADDR";

pub const DEFAULT_ADMIN: &str = "127.0.0.1:9884";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub addr: String,
    pub admin: String,
    pub users: String,
    pub journal: Option<PathBuf>,
    pub certs: PathBuf,
    pub timing: Timing,
    pub log: Option<LogSink>,
    pub trail: Trail,
    pub tls: Option<Tls>,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            addr: CHAT.default.to_string(),
            admin: DEFAULT_ADMIN.to_string(),
            users: USERS.default.to_string(),
            journal: None,
            certs: PathBuf::from(DEFAULT_CERTS),
            timing: Timing::default(),
            log: None,
            trail: Trail::quiet(),
            tls: None,
        }
    }
}

impl Config {
    pub fn from_source(source: &Source) -> Result<Config, ConfigError> {
        let mut fields = source.fields();
        let config = Config {
            addr: fields.addr(CHAT.env, CHAT.default),
            admin: fields.addr(ADMIN_ENV, DEFAULT_ADMIN),
            users: fields.addr(USERS.env, USERS.default),
            journal: fields.optional(JOURNAL_ENV),
            certs: fields.parse(CERTS_ENV, PathBuf::from(DEFAULT_CERTS)),
            timing: Timing::read(&mut fields),
            log: fields.flag(ACCESS_LOG_ENV, true).then(LogSink::stderr),
            trail: Trail::stderr(NAME),
            tls: None,
        };
        fields.distinct_addrs(&[(CHAT.env, &config.addr), (ADMIN_ENV, &config.admin)]);
        fields.finish()?;
        Ok(config)
    }

    pub fn lifecycle(&self, shutdown: &CancellationToken) -> Lifecycle {
        Lifecycle::new(shutdown, Health::default(), self.trail.clone()).timing(self.timing)
    }
}
