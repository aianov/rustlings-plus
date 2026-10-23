use std::time::Duration;

pub const NATS_URL_ENV: &str = "NATS_URL";

pub const DEFAULT_NATS_URL: &str = "nats://127.0.0.1:54222";

const SUBSCRIPTION_CAPACITY: usize = 4096;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BusConfig {
    pub url: String,
    pub name: Option<String>,
    pub subscription_capacity: usize,
    pub connect_timeout: Duration,
}

impl BusConfig {
    pub fn new(url: impl Into<String>) -> BusConfig {
        BusConfig {
            url: url.into(),
            name: None,
            subscription_capacity: SUBSCRIPTION_CAPACITY,
            connect_timeout: CONNECT_TIMEOUT,
        }
    }

    pub fn named(self, name: &str) -> BusConfig {
        BusConfig {
            name: Some(name.to_string()),
            ..self
        }
    }
}

impl Default for BusConfig {
    fn default() -> BusConfig {
        BusConfig::new(DEFAULT_NATS_URL)
    }
}
