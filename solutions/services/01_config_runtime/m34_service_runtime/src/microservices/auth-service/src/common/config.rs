use std::path::PathBuf;

use grpc_kit::{ACCESS_LOG_ENV, CERTS_ENV, DEFAULT_CERTS, LogSink, Tls};
use http_engine::ServerConfig;
use proto::{AUTH, USERS};
use service_runtime::{CancellationToken, ConfigError, Health, Lifecycle, Source, Timing, Trail};

pub const NAME: &str = "auth-service";

pub const HTTP_ADDR_ENV: &str = "AUTH_HTTP_ADDR";

pub const DEFAULT_HTTP_ADDR: &str = "127.0.0.1:7881";

pub const JOURNAL_ENV: &str = "AUTH_JOURNAL";

pub const CORS_ENV: &str = "AUTH_CORS_ORIGINS";

pub const ADMIN_ENV: &str = "AUTH_ADMIN_ADDR";

pub const DEFAULT_ADMIN: &str = "127.0.0.1:9881";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub addr: String,
    pub http_addr: String,
    pub admin: String,
    pub users: String,
    pub journal: Option<PathBuf>,
    pub certs: PathBuf,
    pub http: ServerConfig,
    pub access_log: bool,
    pub cors_origins: Vec<String>,
    pub timing: Timing,
    pub log: Option<LogSink>,
    pub trail: Trail,
    pub tls: Option<Tls>,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            addr: AUTH.default.to_string(),
            http_addr: DEFAULT_HTTP_ADDR.to_string(),
            admin: DEFAULT_ADMIN.to_string(),
            users: USERS.default.to_string(),
            journal: None,
            certs: PathBuf::from(DEFAULT_CERTS),
            http: ServerConfig::default(),
            access_log: false,
            cors_origins: Vec::new(),
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
        let access_log = fields.flag(ACCESS_LOG_ENV, true);
        let config = Config {
            addr: fields.addr(AUTH.env, AUTH.default),
            http_addr: fields.addr(HTTP_ADDR_ENV, DEFAULT_HTTP_ADDR),
            admin: fields.addr(ADMIN_ENV, DEFAULT_ADMIN),
            users: fields.addr(USERS.env, USERS.default),
            journal: fields.optional(JOURNAL_ENV),
            certs: fields.parse(CERTS_ENV, PathBuf::from(DEFAULT_CERTS)),
            http: ServerConfig::default(),
            access_log,
            cors_origins: fields.list(CORS_ENV),
            timing: Timing::read(&mut fields),
            log: access_log.then(LogSink::stderr),
            trail: Trail::stderr(NAME),
            tls: None,
        };
        fields.distinct_addrs(&[
            (AUTH.env, &config.addr),
            (HTTP_ADDR_ENV, &config.http_addr),
            (ADMIN_ENV, &config.admin),
        ]);
        fields.finish()?;
        Ok(config)
    }

    pub fn lifecycle(&self, shutdown: &CancellationToken) -> Lifecycle {
        Lifecycle::new(shutdown, Health::default(), self.trail.clone()).timing(self.timing)
    }
}
