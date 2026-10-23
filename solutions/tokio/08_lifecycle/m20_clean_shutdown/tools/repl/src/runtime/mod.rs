use std::time::Duration;

use tokio::runtime::{Builder, Runtime};

use crate::error::{Error, Result};

pub const CORE_THREADS: usize = 2;

pub const CLOSE_DEADLINE: Duration = Duration::from_secs(5);

pub fn start() -> Result<Runtime> {
    Builder::new_multi_thread()
        .worker_threads(CORE_THREADS)
        .thread_name("repl-core")
        .enable_all()
        .build()
        .map_err(Error::Runtime)
}
