use std::sync::Arc;
use std::time::Duration;

use jobs::{Failure, Handler, Job, Leader, SagaError, now_ms};
use service_runtime::CancellationToken;

use crate::common::NAME;
use crate::service::{AuthService, Deletion};

const TICK: Duration = Duration::from_millis(500);

pub fn deletions(service: Arc<AuthService>, deletion: Arc<Deletion>) -> Handler {
    Arc::new(move |job: Job| {
        let (service, deletion) = (Arc::clone(&service), Arc::clone(&deletion));
        Box::pin(async move {
            let Some(user) = job
                .payload
                .get("user_id")
                .and_then(serde_json::Value::as_u64)
            else {
                return Err(Failure::Permanent(format!(
                    "в задаче {} нет user_id",
                    job.id
                )));
            };
            match service.delete_account(&job.id, user, &deletion).await {
                Ok(()) => Ok(()),
                Err(SagaError::Retry(failure)) => Err(failure),
                Err(SagaError::Store(reason)) => Err(Failure::Transient(reason)),
                Err(error @ SagaError::RolledBack { .. }) => {
                    eprintln!("{NAME}: удаление {user} откачено: {error}");
                    Err(Failure::Permanent(error.to_string()))
                }
            }
        })
    })
}

pub async fn sweep(service: Arc<AuthService>, mut leader: Leader, closing: CancellationToken) {
    let mut ticker = tokio::time::interval(TICK);
    loop {
        tokio::select! {
            () = closing.cancelled() => return,
            _ = ticker.tick() => {}
        }
        let service = Arc::clone(&service);
        let ran = leader
            .tick(now_ms(), |_| async move {
                service.purge().await;
            })
            .await;
        if let Err(error) = ran {
            eprintln!("{NAME}: аренда чистки сессий недоступна: {error}");
        }
    }
}
