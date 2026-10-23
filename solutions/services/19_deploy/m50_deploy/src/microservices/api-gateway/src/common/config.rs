use std::path::PathBuf;
use std::time::Duration;

use contracts::{DEFAULT_ROOT, ROOT_ENV, Topics};

use super::guards::{
    IP_BURST_ENV, IP_RATE_ENV, PROFILE_FRESH, PROFILE_FRESH_ENV, RateLimits, SHED_ENV,
    USER_BURST_ENV, USER_RATE_ENV,
};
use super::links::{
    DEFAULT_LINK_KEY, LINK_KEY_ENV, LINK_TTL, LINK_TTL_ENV, UPLOAD_BUDGET, UPLOAD_BUDGET_ENV,
};
use grpc_kit::{CERTS_ENV, DEFAULT_CERTS, Tls};
use http_engine::ServerConfig;
use nats_engine::{BusConfig, NATS_URL_ENV, token};
use proto::{AUTH, AUTH_HTTP, CHAT, FEED, POSTS, USERS};
use service_runtime::{
    CancellationToken, ConfigError, Fields, Health, Lifecycle, Source, Timing, Trail,
};

pub const NAME: &str = "api-gateway";

pub const HTTP_ADDR_ENV: &str = "GATEWAY_HTTP_ADDR";

pub const POSTS_REPLICAS_ENV: &str = "GATEWAY_POSTS_REPLICAS";

pub const DEFAULT_HTTP_ADDR: &str = "127.0.0.1:7880";

pub const ADMIN_ENV: &str = "GATEWAY_ADMIN_ADDR";

pub const DEFAULT_ADMIN: &str = "127.0.0.1:9880";

pub const CORS_ENV: &str = "GATEWAY_CORS_ORIGINS";

pub const MAX_CONNECTIONS_ENV: &str = "GATEWAY_MAX_CONNECTIONS";

pub const BUDGET_ENV: &str = "GATEWAY_BUDGET_MS";

pub const BUDGET: Duration = Duration::from_millis(300);

pub const MAX_IN_FLIGHT_ENV: &str = "GATEWAY_MAX_IN_FLIGHT";

pub const NOTIFY_TIMEOUT_ENV: &str = "GATEWAY_NOTIFY_TIMEOUT_MS";

pub const NOTIFY_TIMEOUT: Duration = Duration::from_millis(1000);

pub const PART_BUDGET_ENV: &str = "GATEWAY_PART_BUDGET_MS";

pub const PART_BUDGET: Duration = Duration::from_millis(150);

pub const PROXY_TIMEOUT_ENV: &str = "GATEWAY_PROXY_TIMEOUT_MS";

pub const PROXY_BODY_ENV: &str = "GATEWAY_PROXY_BODY_MAX";

pub const PRESENCE_TTL_ENV: &str = "GATEWAY_PRESENCE_TTL_MS";

pub const PRESENCE_TTL: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProxyConfig {
    pub max_body: usize,
    pub timeout: Duration,
}

impl Default for ProxyConfig {
    fn default() -> ProxyConfig {
        ProxyConfig {
            max_body: 16 * 1024,
            timeout: Duration::from_secs(5),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Services {
    pub auth: String,
    pub auth_http: String,
    pub users: String,
    pub posts: String,
    pub chat: String,
    pub feed: String,
}

impl Default for Services {
    fn default() -> Services {
        Services {
            auth: AUTH.default.to_string(),
            auth_http: AUTH_HTTP.default.to_string(),
            users: USERS.default.to_string(),
            posts: POSTS.default.to_string(),
            chat: CHAT.default.to_string(),
            feed: FEED.default.to_string(),
        }
    }
}

impl Services {
    pub fn read(fields: &mut Fields<'_>) -> Services {
        Services {
            auth: fields.addr(AUTH.env, AUTH.default),
            auth_http: fields.addr(AUTH_HTTP.env, AUTH_HTTP.default),
            users: fields.addr(USERS.env, USERS.default),
            posts: match fields.optional::<String>(POSTS_REPLICAS_ENV) {
                Some(replicas) => replicas,
                None => fields.addr(POSTS.env, POSTS.default),
            },
            chat: fields.addr(CHAT.env, CHAT.default),
            feed: fields.addr(FEED.env, FEED.default),
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
    pub part_budget: Duration,
    pub proxy: ProxyConfig,
    pub http: ServerConfig,
    pub access_log: bool,
    pub cors_origins: Vec<String>,
    pub avatar_max: usize,
    pub link_key: String,
    pub link_ttl: Duration,
    pub upload_budget: Duration,
    pub limits: RateLimits,
    pub profile_fresh: Duration,
    pub timing: Timing,
    pub trail: Trail,
    pub tls: Option<Tls>,
    pub nats: Option<BusConfig>,
    pub topics: Topics,
    pub notify_timeout: Duration,
    pub presence_ttl: Duration,
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
            part_budget: PART_BUDGET,
            proxy: ProxyConfig::default(),
            http: ServerConfig::default(),
            access_log: false,
            cors_origins: Vec::new(),
            avatar_max: 256 * 1024,
            link_key: DEFAULT_LINK_KEY.to_string(),
            link_ttl: LINK_TTL,
            upload_budget: UPLOAD_BUDGET,
            limits: RateLimits::default(),
            profile_fresh: PROFILE_FRESH,
            timing: Timing::default(),
            trail: Trail::quiet(),
            tls: None,
            nats: None,
            topics: Topics::new(DEFAULT_ROOT),
            notify_timeout: NOTIFY_TIMEOUT,
            presence_ttl: PRESENCE_TTL,
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
            max_in_flight: fields.parse(MAX_IN_FLIGHT_ENV, standard.max_in_flight),
            budget: fields.millis(BUDGET_ENV, BUDGET),
            part_budget: fields.millis(PART_BUDGET_ENV, PART_BUDGET),
            proxy: ProxyConfig {
                max_body: fields.parse(PROXY_BODY_ENV, standard.proxy.max_body),
                timeout: fields.millis(PROXY_TIMEOUT_ENV, standard.proxy.timeout),
            },
            access_log: fields.flag(grpc_kit::ACCESS_LOG_ENV, true),
            cors_origins: fields.list(CORS_ENV),
            timing: Timing::read(&mut fields),
            trail: Trail::stderr(NAME),
            nats: fields
                .optional::<String>(NATS_URL_ENV)
                .map(|url| BusConfig::new(url).named(NAME)),
            topics: Topics::new(&fields.text(ROOT_ENV, DEFAULT_ROOT)),
            notify_timeout: fields.millis(NOTIFY_TIMEOUT_ENV, NOTIFY_TIMEOUT),
            presence_ttl: fields.millis(PRESENCE_TTL_ENV, PRESENCE_TTL),
            link_key: fields.text(LINK_KEY_ENV, DEFAULT_LINK_KEY),
            link_ttl: fields.millis(LINK_TTL_ENV, LINK_TTL),
            upload_budget: fields.millis(UPLOAD_BUDGET_ENV, UPLOAD_BUDGET),
            limits: RateLimits {
                user_rate: fields.parse(USER_RATE_ENV, standard.limits.user_rate),
                user_burst: fields.parse(USER_BURST_ENV, standard.limits.user_burst),
                ip_rate: fields.parse(IP_RATE_ENV, standard.limits.ip_rate),
                ip_burst: fields.parse(IP_BURST_ENV, standard.limits.ip_burst),
                shed_inflight: fields.parse(SHED_ENV, standard.limits.shed_inflight),
            },
            profile_fresh: fields.millis(PROFILE_FRESH_ENV, PROFILE_FRESH),
            ..standard
        };
        if token(config.topics.root()).is_err() {
            fields.reject(
                ROOT_ENV,
                "корень тем это одно слово: 1..=32 символа A-Za-z0-9_-",
            );
        }
        if config.notify_timeout.is_zero() {
            fields.reject(
                NOTIFY_TIMEOUT_ENV,
                "с нулём колокольчик никогда не дождётся notify-service",
            );
        }
        if config.presence_ttl < Duration::from_millis(300) {
            fields.reject(
                PRESENCE_TTL_ENV,
                "срок присутствия от 300 мс: сигнал идёт каждую треть срока",
            );
        }
        if config.link_key.len() < 16 {
            fields.reject(LINK_KEY_ENV, "ключ подписи ссылок от 16 байт");
        }
        if config.link_ttl < Duration::from_secs(1) {
            fields.reject(LINK_TTL_ENV, "ссылка живёт хотя бы секунду");
        }
        if config.limits.user_rate == 0 || config.limits.ip_rate == 0 {
            fields.reject(USER_RATE_ENV, "ведро с нулевым притоком закроется навсегда");
        }
        if config.max_connections == 0 {
            fields.reject(MAX_CONNECTIONS_ENV, "нужно хотя бы одно соединение");
        }
        if config.budget.is_zero() {
            fields.reject(BUDGET_ENV, "нулевой бюджет отклонит каждый вызов");
        }
        if config.max_in_flight == 0 {
            fields.reject(
                MAX_IN_FLIGHT_ENV,
                "с нулём соединение не выполнит ни одного запроса",
            );
        }
        if config.part_budget.is_zero() {
            fields.reject(
                PART_BUDGET_ENV,
                "с нулём необязательные части экрана не придут никогда",
            );
        }
        if config.proxy.timeout.is_zero() {
            fields.reject(PROXY_TIMEOUT_ENV, "с нулём вход через шлюз всегда 504");
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
