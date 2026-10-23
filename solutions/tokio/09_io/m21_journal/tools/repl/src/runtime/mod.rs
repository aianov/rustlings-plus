use std::path::Path;
use std::time::Duration;

use social_core::{App, AppConfig, MemNetwork, Notifiers};
use tokio::runtime::{Builder, Runtime};

use crate::error::{Error, Result};

pub const CORE_THREADS: usize = 2;

pub const CLOSE_DEADLINE: Duration = Duration::from_secs(5);

pub const JOURNAL_ENV: &str = "SOCIAL_JOURNAL";

pub fn start() -> Result<Runtime> {
    Builder::new_multi_thread()
        .worker_threads(CORE_THREADS)
        .thread_name("repl-core")
        .enable_all()
        .build()
        .map_err(Error::Runtime)
}

pub async fn open(notifiers: Notifiers, journal: Option<&Path>) -> Result<App> {
    let config = AppConfig::default();
    let app = match journal {
        Some(path) => App::restore(MemNetwork::new(), notifiers, config, path).await?,
        None => App::start(MemNetwork::new(), notifiers, config)?,
    };
    Ok(app)
}
