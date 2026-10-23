use tokio::runtime::{Builder, Runtime};

use crate::error::{Error, Result};

pub const CORE_THREADS: usize = 2;

pub fn start() -> Result<Runtime> {
    Builder::new_multi_thread()
        .worker_threads(CORE_THREADS)
        .thread_name("repl-core")
        .enable_all()
        .build()
        .map_err(Error::Runtime)
}
