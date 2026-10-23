use std::path::PathBuf;
use std::time::Duration;

use grpc_kit::{CERTS_ENV, DEFAULT_CERTS, Tls};
use http_engine::ServerConfig;
use proto::{AUTH, CHAT, POSTS, USERS};
use service_runtime::{
    CancellationToken, ConfigError, Fields, Health, Lifecycle, Source, Timing, Trail,
};

pub const NAME: &str = "api-gateway";

pub const HTTP_ADDR_ENV: &str = "GATEWAY_HTTP_ADDR";

pub const DEFAULT_HTTP_ADDR: &str = "127.0.0.1:7880";

pub const ADMIN_ENV: &str = "GATEWAY_ADMIN_ADDR";

pub const DEFAULT_ADMIN: &str = "127.0.0.1:9880";

pub const CORS_ENV: &str = "GATEWAY_CORS_ORIGINS";

pub const MAX_CONNECTIONS_ENV: &str = "GATEWAY_MAX_CONNECTIONS";

pub const BUDGET_ENV: &str = "GATEWAY_BUDGET_MS";

pub const BUDGET: Duration = Duration::from_millis(300);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Services {
    pub auth: String,
    pub users: String,
    pub posts: String,
    pub chat: String,
}

impl Default for Services {
    fn default() -> Services {
        Services {
            auth: AUTH.default.to_string(),
            users: USERS.default.to_string(),
            posts: POSTS.default.to_string(),
            chat: CHAT.default.to_string(),
        }
    }
}

impl Services {
    pub fn read(fields: &mut Fields<'_>) -> Services {
        Services {
            auth: fields.addr(AUTH.env, AUTH.default),
            users: fields.addr(USERS.env, USERS.default),
            posts: fields.addr(POSTS.env, POSTS.default),
            chat: fields.addr(CHAT.env, CHAT.default),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub http_addr: String,
    pub admin: String,
    pub certs: PathBuf,
    pub max_connections: usize,
    pub idle_timeout: Duration,
    pub ping_every: Duration,
    pub max_in_flight: usize,
    pub max_line: usize,
    pub outbox: usize,
    pub services: Services,
    pub budget: Duration,
    pub http: ServerConfig,
    pub access_log: bool,
    pub cors_origins: Vec<String>,
    pub avatar_max: usize,
    pub timing: Timing,
    pub trail: Trail,
    pub tls: Option<Tls>,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            http_addr: DEFAULT_HTTP_ADDR.to_string(),
            admin: DEFAULT_ADMIN.to_string(),
            certs: PathBuf::from(DEFAULT_CERTS),
            max_connections: 256,
            idle_timeout: Duration::from_secs(90),
            ping_every: Duration::from_secs(30),
            max_in_flight: 8,
            max_line: 64 * 1024,
            outbox: 64,
            services: Services::default(),
            budget: BUDGET,
            http: ServerConfig::default(),
            access_log: false,
            cors_origins: Vec::new(),
            avatar_max: 256 * 1024,
            timing: Timing::default(),
            trail: Trail::quiet(),
            tls: None,
        }
    }
}

impl Config {
    pub fn from_source(source: &Source) -> Result<Config, ConfigError> {
        let mut fields = source.fields();
        let standard = Config::default();
        let config = Config {
            http_addr: fields.addr(HTTP_ADDR_ENV, DEFAULT_HTTP_ADDR),
            admin: fields.addr(ADMIN_ENV, DEFAULT_ADMIN),
            certs: fields.parse(CERTS_ENV, PathBuf::from(DEFAULT_CERTS)),
            max_connections: fields.parse(MAX_CONNECTIONS_ENV, standard.max_connections),
            services: Services::read(&mut fields),
            budget: fields.millis(BUDGET_ENV, BUDGET),
            access_log: fields.flag(grpc_kit::ACCESS_LOG_ENV, true),
            cors_origins: fields.list(CORS_ENV),
            timing: Timing::read(&mut fields),
            trail: Trail::stderr(NAME),
            ..standard
        };
        if config.max_connections == 0 {
            fields.reject(MAX_CONNECTIONS_ENV, "нужно хотя бы одно соединение");
        }
        if config.budget.is_zero() {
            fields.reject(BUDGET_ENV, "нулевой бюджет отклонит каждый вызов");
        }
        fields.distinct_addrs(&[
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
