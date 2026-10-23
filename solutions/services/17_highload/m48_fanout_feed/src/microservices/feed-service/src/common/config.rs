use std::path::PathBuf;

use contracts::{DEFAULT_ROOT, ROOT_ENV, Topics};
use grpc_kit::{ACCESS_LOG_ENV, CERTS_ENV, DEFAULT_CERTS, LogSink, Tls};
use nats_engine::{BusConfig, DEFAULT_NATS_URL, NATS_URL_ENV, token};
use proto::FEED;

use super::limits::{CELEBRITY_ENV, FEED_MAX_ENV, FeedLimits, INLINE_ENV};
use service_runtime::{CancellationToken, ConfigError, Health, Lifecycle, Source, Timing, Trail};

pub const NAME: &str = "feed-service";

pub const ADMIN_ENV: &str = "FEED_ADMIN_ADDR";

pub const DEFAULT_ADMIN: &str = "127.0.0.1:9886";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub addr: String,
    pub admin: String,
    pub nats: BusConfig,
    pub topics: Topics,
    pub certs: PathBuf,
    pub timing: Timing,
    pub log: Option<LogSink>,
    pub trail: Trail,
    pub tls: Option<Tls>,
    pub limits: FeedLimits,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            addr: FEED.default.to_string(),
            admin: DEFAULT_ADMIN.to_string(),
            nats: BusConfig::new(DEFAULT_NATS_URL).named(NAME),
            topics: Topics::new(DEFAULT_ROOT),
            certs: PathBuf::from(DEFAULT_CERTS),
            timing: Timing::default(),
            log: None,
            trail: Trail::quiet(),
            tls: None,
            limits: FeedLimits::default(),
        }
    }
}

impl Config {
    pub fn from_source(source: &Source) -> Result<Config, ConfigError> {
        let mut fields = source.fields();
        let config = Config {
            addr: fields.addr(FEED.env, FEED.default),
            admin: fields.addr(ADMIN_ENV, DEFAULT_ADMIN),
            nats: BusConfig::new(fields.text(NATS_URL_ENV, DEFAULT_NATS_URL)).named(NAME),
            topics: Topics::new(&fields.text(ROOT_ENV, DEFAULT_ROOT)),
            certs: fields.parse(CERTS_ENV, PathBuf::from(DEFAULT_CERTS)),
            timing: Timing::read(&mut fields),
            log: fields.flag(ACCESS_LOG_ENV, true).then(LogSink::stderr),
            trail: Trail::stderr(NAME),
            tls: None,
            limits: FeedLimits {
                max: fields.parse(FEED_MAX_ENV, FeedLimits::default().max),
                celebrity: fields.parse(CELEBRITY_ENV, FeedLimits::default().celebrity),
                inline: fields.parse(INLINE_ENV, FeedLimits::default().inline),
            },
        };
        if config.limits.max == 0 || config.limits.inline == 0 || config.limits.celebrity == 0 {
            fields.reject(
                FEED_MAX_ENV,
                "длина ленты, раскладка сразу и порог знаменитости больше нуля",
            );
        }
        if token(config.topics.root()).is_err() {
            fields.reject(
                ROOT_ENV,
                "корень тем это одно слово: 1..=32 символа A-Za-z0-9_-",
            );
        }
        fields.distinct_addrs(&[(FEED.env, &config.addr), (ADMIN_ENV, &config.admin)]);
        fields.finish()?;
        Ok(config)
    }

    pub fn lifecycle(&self, shutdown: &CancellationToken) -> Lifecycle {
        Lifecycle::new(shutdown, Health::default(), self.trail.clone()).timing(self.timing)
    }
}
