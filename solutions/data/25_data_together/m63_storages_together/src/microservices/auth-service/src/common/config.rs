use crate::repository::Stores;
use db_engine::DbConfig;
use resp_client::{REDIS_URL_ENV, RedisConfig};
use std::path::PathBuf;

use contracts::{DEFAULT_ROOT, ROOT_ENV, Topics};
use grpc_kit::{ACCESS_LOG_ENV, CERTS_ENV, DEFAULT_CERTS, LogSink, Tls};
use http_engine::ServerConfig;
use nats_engine::{BusConfig, NATS_URL_ENV, token};
use proto::{AUTH, POSTS, USERS};

use crate::service::{Cost, KeyRing, Lifetimes};
use service_runtime::{CancellationToken, ConfigError, Health, Lifecycle, Source, Timing, Trail};

pub const NAME: &str = "auth-service";

pub const HTTP_ADDR_ENV: &str = "AUTH_HTTP_ADDR";

pub const DEFAULT_HTTP_ADDR: &str = "127.0.0.1:7881";

pub const CORS_ENV: &str = "AUTH_CORS_ORIGINS";

pub const ADMIN_ENV: &str = "AUTH_ADMIN_ADDR";

pub const DEFAULT_ADMIN: &str = "127.0.0.1:9881";

pub const KEYS_ENV: &str = "AUTH_SIGNING_KEYS";

pub const ACCESS_TTL_ENV: &str = "AUTH_ACCESS_TTL_SECS";

pub const REFRESH_TTL_ENV: &str = "AUTH_REFRESH_TTL_SECS";

pub const ARGON_MEMORY_ENV: &str = "AUTH_ARGON_MEMORY_KIB";

pub const ARGON_ITERATIONS_ENV: &str = "AUTH_ARGON_ITERATIONS";

pub const HASH_PARALLEL_ENV: &str = "AUTH_HASH_PARALLEL";

pub const MODERATORS_ENV: &str = "AUTH_MODERATORS";

pub const PUBLIC_URL_ENV: &str = "AUTH_PUBLIC_URL";

pub const LEEWAY_ENV: &str = "AUTH_CLOCK_LEEWAY_SECS";

pub const SWEEP_EVERY_ENV: &str = "AUTH_SWEEP_EVERY_MS";

pub const SWEEP_EVERY: std::time::Duration = std::time::Duration::from_secs(60);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub addr: String,
    pub http_addr: String,
    pub admin: String,
    pub users: String,
    pub posts: String,
    pub sweep_every: std::time::Duration,
    pub stores: Option<Stores>,
    pub certs: PathBuf,
    pub http: ServerConfig,
    pub access_log: bool,
    pub cors_origins: Vec<String>,
    pub timing: Timing,
    pub keys: Option<KeyRing>,
    pub lifetimes: Lifetimes,
    pub cost: Cost,
    pub hash_parallel: usize,
    pub moderators: Vec<String>,
    pub public_url: String,
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
            addr: AUTH.default.to_string(),
            http_addr: DEFAULT_HTTP_ADDR.to_string(),
            admin: DEFAULT_ADMIN.to_string(),
            users: USERS.default.to_string(),
            posts: POSTS.default.to_string(),
            sweep_every: SWEEP_EVERY,
            stores: Some(Stores::memory(Lifetimes::default().leeway)),
            certs: PathBuf::from(DEFAULT_CERTS),
            http: ServerConfig::default(),
            access_log: false,
            cors_origins: Vec::new(),
            timing: Timing::default(),
            keys: None,
            lifetimes: Lifetimes::default(),
            cost: Cost::QUICK,
            hash_parallel: 4,
            moderators: Vec::new(),
            public_url: format!("https://{DEFAULT_HTTP_ADDR}"),
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
        let access_log = fields.flag(ACCESS_LOG_ENV, true);
        let keys = fields.secret_optional(KEYS_ENV);
        let parsed = keys.as_ref().map(|spec| KeyRing::parse(spec.expose()));
        if let Some(Err(reason)) = &parsed {
            fields.reject_secret(KEYS_ENV, reason);
        }
        let standard = Lifetimes::default();
        let lifetimes = Lifetimes {
            access: fields.parse(ACCESS_TTL_ENV, standard.access),
            refresh: fields.parse(REFRESH_TTL_ENV, standard.refresh),
            leeway: fields.parse(LEEWAY_ENV, standard.leeway),
            ..standard
        };
        if lifetimes.access == 0 || lifetimes.access >= lifetimes.refresh {
            fields.reject(ACCESS_TTL_ENV, "access живёт меньше refresh и больше нуля");
        }
        let cost = Cost {
            memory_kib: fields.parse(ARGON_MEMORY_ENV, Cost::STRONG.memory_kib),
            iterations: fields.parse(ARGON_ITERATIONS_ENV, Cost::STRONG.iterations),
            ..Cost::STRONG
        };
        let http_addr = fields.addr(HTTP_ADDR_ENV, DEFAULT_HTTP_ADDR);
        let config = Config {
            addr: fields.addr(AUTH.env, AUTH.default),
            public_url: fields.text(PUBLIC_URL_ENV, &format!("https://{http_addr}")),
            http_addr,
            admin: fields.addr(ADMIN_ENV, DEFAULT_ADMIN),
            users: fields.addr(USERS.env, USERS.default),
            posts: fields.addr(POSTS.env, POSTS.default),
            sweep_every: fields.millis(SWEEP_EVERY_ENV, SWEEP_EVERY),
            stores: None,
            certs: fields.parse(CERTS_ENV, PathBuf::from(DEFAULT_CERTS)),
            http: ServerConfig::default(),
            access_log,
            cors_origins: fields.list(CORS_ENV),
            timing: Timing::read(&mut fields),
            keys: parsed.and_then(Result::ok),
            lifetimes,
            cost,
            hash_parallel: fields.parse(HASH_PARALLEL_ENV, 4),
            moderators: fields.list(MODERATORS_ENV),
            log: access_log.then(LogSink::stderr),
            trail: Trail::stderr(NAME),
            tls: None,
            nats: fields
                .optional::<String>(NATS_URL_ENV)
                .map(|url| BusConfig::new(url).named(NAME)),
            topics: Topics::new(&fields.text(ROOT_ENV, DEFAULT_ROOT)),
            database: DbConfig::read(&mut fields, "AUTH", "auth"),
            redis: fields
                .optional::<String>("AUTH_REDIS_URL")
                .or_else(|| fields.optional::<String>(REDIS_URL_ENV))
                .map(|addr| RedisConfig::new(&addr)),
        };
        if token(config.topics.root()).is_err() {
            fields.reject(
                ROOT_ENV,
                "корень тем это одно слово: 1..=32 символа A-Za-z0-9_-",
            );
        }
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
