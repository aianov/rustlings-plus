use std::time::Duration;

use crate::shards::DEFAULT_SHARDS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppConfig {
    pub op_timeout: Duration,
    pub heavy_limit: usize,
    pub import_window: usize,
    pub trends_every: Duration,
    pub trends_debounce: Duration,
    pub shards: usize,
    pub events: usize,
    pub queue: usize,
    pub delivery_workers: usize,
    pub batch_size: usize,
    pub batch_window: Duration,
    pub restart_pause: Duration,
    pub max_restarts: u32,
    pub journal_flush_every: usize,
    pub journal_max_line: usize,
}

impl Default for AppConfig {
    fn default() -> AppConfig {
        AppConfig {
            op_timeout: Duration::from_secs(5),
            heavy_limit: 2,
            import_window: 8,
            trends_every: Duration::from_secs(60),
            trends_debounce: Duration::from_secs(2),
            shards: DEFAULT_SHARDS,
            events: 256,
            queue: 1024,
            delivery_workers: 4,
            batch_size: 32,
            batch_window: Duration::from_millis(50),
            restart_pause: Duration::from_millis(100),
            max_restarts: 5,
            journal_flush_every: 32,
            journal_max_line: 64 * 1024,
        }
    }
}
