use db_engine::DbConfig;
use std::path::PathBuf;

use contracts::{DEFAULT_ROOT, ROOT_ENV, Topics};
use grpc_kit::{ACCESS_LOG_ENV, CERTS_ENV, DEFAULT_CERTS, LogSink, Tls};
use nats_engine::{BusConfig, NATS_URL_ENV, token};
use proto::{POSTS, USERS};
use service_runtime::{CancellationToken, ConfigError, Health, Lifecycle, Source, Timing, Trail};

pub const NAME: &str = "post-service";

pub const JOURNAL_ENV: &str = "POSTS_JOURNAL";

pub const ADMIN_ENV: &str = "POSTS_ADMIN_ADDR";

pub const DEFAULT_ADMIN: &str = "127.0.0.1:9883";

pub const NODE_ENV: &str = "POSTS_NODE";

pub const PREVIEW_ENV: &str = "POSTS_PREVIEW";

pub const LIKE_FLUSH: std::time::Duration = std::time::Duration::from_millis(100);

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
    pub nats: Option<BusConfig>,
    pub topics: Topics,
    pub node: u16,
    pub previews: bool,
    pub database: Option<DbConfig>,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            addr: POSTS.default.to_string(),
            admin: DEFAULT_ADMIN.to_string(),
            users: USERS.default.to_string(),
            journal: None,
            certs: PathBuf::from(DEFAULT_CERTS),
            timing: Timing::default(),
            log: None,
            trail: Trail::quiet(),
            tls: None,
            nats: None,
            topics: Topics::new(DEFAULT_ROOT),
            node: 1,
            previews: false,
            database: None,
        }
    }
}

impl Config {
    pub fn from_source(source: &Source) -> Result<Config, ConfigError> {
        let mut fields = source.fields();
        let config = Config {
            addr: fields.addr(POSTS.env, POSTS.default),
            admin: fields.addr(ADMIN_ENV, DEFAULT_ADMIN),
            users: fields.addr(USERS.env, USERS.default),
            journal: fields.optional(JOURNAL_ENV),
            certs: fields.parse(CERTS_ENV, PathBuf::from(DEFAULT_CERTS)),
            timing: Timing::read(&mut fields),
            log: fields.flag(ACCESS_LOG_ENV, true).then(LogSink::stderr),
            trail: Trail::stderr(NAME),
            tls: None,
            nats: fields
                .optional::<String>(NATS_URL_ENV)
                .map(|url| BusConfig::new(url).named(NAME)),
            topics: Topics::new(&fields.text(ROOT_ENV, DEFAULT_ROOT)),
            node: fields.parse(NODE_ENV, 1),
            previews: fields.flag(PREVIEW_ENV, false),
            database: DbConfig::read(&mut fields, "POSTS", "posts"),
        };
        if token(config.topics.root()).is_err() {
            fields.reject(
                ROOT_ENV,
                "корень тем это одно слово: 1..=32 символа A-Za-z0-9_-",
            );
        }
        if config.node > service_runtime::MAX_NODE {
            fields.reject(NODE_ENV, "номер узла в id поста это 10 бит: 0..=1023");
        }
        fields.distinct_addrs(&[(POSTS.env, &config.addr), (ADMIN_ENV, &config.admin)]);
        fields.finish()?;
        Ok(config)
    }

    pub fn lifecycle(&self, shutdown: &CancellationToken) -> Lifecycle {
        Lifecycle::new(shutdown, Health::default(), self.trail.clone()).timing(self.timing)
    }
}
