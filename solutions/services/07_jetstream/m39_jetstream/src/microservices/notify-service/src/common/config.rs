use std::path::PathBuf;
use std::time::Duration;

use contracts::{DEFAULT_ROOT, NOTIFY_GROUP, ROOT_ENV, Topics};
use grpc_kit::{CERTS_ENV, DEFAULT_CERTS, Tls};
use nats_engine::{BusConfig, DEFAULT_NATS_URL, NATS_URL_ENV, token};
use proto::USERS;
use service_runtime::{CancellationToken, ConfigError, Health, Lifecycle, Source, Timing, Trail};

pub const NAME: &str = "notify-service";

pub const ADMIN_ENV: &str = "NOTIFY_ADMIN_ADDR";

pub const DEFAULT_ADMIN: &str = "127.0.0.1:9885";

pub const WORKERS_ENV: &str = "NOTIFY_WORKERS";

pub const ACK_WAIT_ENV: &str = "NOTIFY_ACK_WAIT_MS";

pub const BATCH_ENV: &str = "NOTIFY_BATCH";

pub const AUDIENCE_TIMEOUT_ENV: &str = "NOTIFY_AUDIENCE_TIMEOUT_MS";

pub const DEFAULT_WORKERS: usize = 4;

pub const DEFAULT_ACK_WAIT: Duration = Duration::from_secs(5);

pub const DEFAULT_BATCH: usize = 32;

pub const DEFAULT_AUDIENCE_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub admin: String,
    pub users: String,
    pub nats: BusConfig,
    pub topics: Topics,
    pub group: String,
    pub workers: usize,
    pub batch: usize,
    pub ack_wait: Duration,
    pub audience_timeout: Duration,
    pub certs: PathBuf,
    pub timing: Timing,
    pub trail: Trail,
    pub tls: Option<Tls>,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            admin: DEFAULT_ADMIN.to_string(),
            users: USERS.default.to_string(),
            nats: BusConfig::new(DEFAULT_NATS_URL).named(NAME),
            topics: Topics::new(DEFAULT_ROOT),
            group: NOTIFY_GROUP.to_string(),
            workers: DEFAULT_WORKERS,
            batch: DEFAULT_BATCH,
            ack_wait: DEFAULT_ACK_WAIT,
            audience_timeout: DEFAULT_AUDIENCE_TIMEOUT,
            certs: PathBuf::from(DEFAULT_CERTS),
            timing: Timing::default(),
            trail: Trail::quiet(),
            tls: None,
        }
    }
}

impl Config {
    pub fn from_source(source: &Source) -> Result<Config, ConfigError> {
        let mut fields = source.fields();
        let config = Config {
            admin: fields.addr(ADMIN_ENV, DEFAULT_ADMIN),
            users: fields.addr(USERS.env, USERS.default),
            nats: BusConfig::new(fields.text(NATS_URL_ENV, DEFAULT_NATS_URL)).named(NAME),
            topics: Topics::new(&fields.text(ROOT_ENV, DEFAULT_ROOT)),
            group: NOTIFY_GROUP.to_string(),
            workers: fields.parse(WORKERS_ENV, DEFAULT_WORKERS),
            batch: fields.parse(BATCH_ENV, DEFAULT_BATCH),
            ack_wait: fields.millis(ACK_WAIT_ENV, DEFAULT_ACK_WAIT),
            audience_timeout: fields.millis(AUDIENCE_TIMEOUT_ENV, DEFAULT_AUDIENCE_TIMEOUT),
            certs: fields.parse(CERTS_ENV, PathBuf::from(DEFAULT_CERTS)),
            timing: Timing::read(&mut fields),
            trail: Trail::stderr(NAME),
            tls: None,
        };
        if token(config.topics.root()).is_err() {
            fields.reject(
                ROOT_ENV,
                "корень тем это одно слово: 1..=32 символа A-Za-z0-9_-",
            );
        }
        if config.workers == 0 {
            fields.reject(WORKERS_ENV, "нужен хотя бы один работник");
        }
        if config.batch == 0 {
            fields.reject(BATCH_ENV, "пачка из нуля событий ничего не прочитает");
        }
        if config.ack_wait.is_zero() {
            fields.reject(
                ACK_WAIT_ENV,
                "с нулём брокер вернёт каждое событие сразу же",
            );
        }
        if config.audience_timeout.is_zero() {
            fields.reject(AUDIENCE_TIMEOUT_ENV, "срок запроса подписчиков больше нуля");
        }
        fields.finish()?;
        Ok(config)
    }

    pub fn lifecycle(&self, shutdown: &CancellationToken) -> Lifecycle {
        Lifecycle::new(shutdown, Health::default(), self.trail.clone()).timing(self.timing)
    }
}
