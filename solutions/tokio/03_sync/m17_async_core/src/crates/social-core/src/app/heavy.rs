use std::sync::Arc;

use tokio::time::timeout;

use super::{App, AppError, Op};
use crate::ports::{PostStore, UserStore};

impl<U, P> App<U, P>
where
    U: UserStore + Send + 'static,
    P: PostStore + Send + 'static,
{
    pub async fn run_heavy<T: Send + 'static>(
        &self,
        op: Op,
        job: impl FnOnce() -> T + Send + 'static,
    ) -> Result<T, AppError> {
        let after = self.inner.config.op_timeout;
        let late = AppError::Timeout { op, after };
        let permit = timeout(after, Arc::clone(&self.inner.heavy).acquire_owned())
            .await
            .map_err(|_| late.clone())?
            .map_err(|_| AppError::Stopped(op))?;
        let task = self.inner.runtime.spawn_blocking(move || {
            let _permit = permit;
            job()
        });
        match timeout(after, task).await {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(_)) => Err(AppError::Panicked(op)),
            Err(_) => Err(late),
        }
    }
}
