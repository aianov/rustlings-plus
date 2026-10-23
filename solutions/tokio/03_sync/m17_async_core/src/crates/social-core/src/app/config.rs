use std::time::Duration;

use crate::shards::DEFAULT_SHARDS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppConfig {
    pub op_timeout: Duration,
    pub heavy_limit: usize,
    pub import_window: usize,
    pub trends_every: Duration,
    pub shards: usize,
}

impl Default for AppConfig {
    fn default() -> AppConfig {
        AppConfig {
            op_timeout: Duration::from_secs(5),
            heavy_limit: 2,
            import_window: 8,
            trends_every: Duration::from_secs(60),
            shards: DEFAULT_SHARDS,
        }
    }
}
