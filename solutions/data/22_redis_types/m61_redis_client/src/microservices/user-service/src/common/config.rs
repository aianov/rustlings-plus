use db_engine::DbConfig;
use resp_client::{REDIS_URL_ENV, RedisConfig};
use std::path::PathBuf;

use contracts::{DEFAULT_ROOT, ROOT_ENV, Topics};
use grpc_kit::{ACCESS_LOG_ENV, CERTS_ENV, DEFAULT_CERTS, LogSink, Tls};
use nats_engine::{BusConfig, NATS_URL_ENV, token};
use proto::USERS;
use service_runtime::{CancellationToken, ConfigError, Health, Lifecycle, Source, Timing, Trail};

pub const NAME: &str = "user-service";

pub const JOURNAL_ENV: &str = "USERS_JOURNAL";

pub const FILES_ENV: &str = "USERS_FILES_DIR";

pub const ORPHAN_SWEEP_ENV: &str = "USERS_ORPHAN_SWEEP_MS";

pub const ORPHAN_SWEEP: std::time::Duration = std::time::Duration::from_secs(10 * 60);

pub const ADMIN_ENV: &str = "USERS_ADMIN_ADDR";

pub const DEFAULT_ADMIN: &str = "127.0.0.1:9882";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub addr: String,
    pub admin: String,
    pub journal: Option<PathBuf>,
    pub files: PathBuf,
    pub orphan_sweep: std::time::Duration,
    pub certs: PathBuf,
    pub timing: Timing,
    pub log: Option<LogSink>,
    pub trail: Trail,
    pub tls: Option<Tls>,
    pub nats: Option<BusConfig>,
    pub topics: Topics,
    pub database: Option<DbConfig>,
    pub redis: Option<RedisConfig>,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            addr: USERS.default.to_string(),
            admin: DEFAULT_ADMIN.to_string(),
            journal: None,
            files: std::env::temp_dir().join("social-files"),
            orphan_sweep: ORPHAN_SWEEP,
            certs: PathBuf::from(DEFAULT_CERTS),
            timing: Timing::default(),
            log: None,
            trail: Trail::quiet(),
            tls: None,
            nats: None,
            topics: Topics::new(DEFAULT_ROOT),
            database: None,
            redis: None,
        }
    }
}

impl Config {
    pub fn from_source(source: &Source) -> Result<Config, ConfigError> {
        let mut fields = source.fields();
        let config = Config {
            addr: fields.addr(USERS.env, USERS.default),
            admin: fields.addr(ADMIN_ENV, DEFAULT_ADMIN),
            journal: fields.optional(JOURNAL_ENV),
            files: fields.parse(FILES_ENV, std::env::temp_dir().join("social-files")),
            orphan_sweep: fields.millis(ORPHAN_SWEEP_ENV, ORPHAN_SWEEP),
            certs: fields.parse(CERTS_ENV, PathBuf::from(DEFAULT_CERTS)),
            timing: Timing::read(&mut fields),
            log: fields.flag(ACCESS_LOG_ENV, true).then(LogSink::stderr),
            trail: Trail::stderr(NAME),
            tls: None,
            nats: fields
                .optional::<String>(NATS_URL_ENV)
                .map(|url| BusConfig::new(url).named(NAME)),
            topics: Topics::new(&fields.text(ROOT_ENV, DEFAULT_ROOT)),
            database: DbConfig::read(&mut fields, "USERS", "users"),
            redis: fields
                .optional::<String>("USERS_REDIS_URL")
                .or_else(|| fields.optional::<String>(REDIS_URL_ENV))
                .map(|addr| RedisConfig::new(&addr)),
        };
        if token(config.topics.root()).is_err() {
            fields.reject(
                ROOT_ENV,
                "корень тем это одно слово: 1..=32 символа A-Za-z0-9_-",
            );
        }
        fields.distinct_addrs(&[(USERS.env, &config.addr), (ADMIN_ENV, &config.admin)]);
        fields.finish()?;
        Ok(config)
    }

    pub fn lifecycle(&self, shutdown: &CancellationToken) -> Lifecycle {
        Lifecycle::new(shutdown, Health::default(), self.trail.clone()).timing(self.timing)
    }
}
