use std::future::Future;

use nats_engine::js::{Bucket, open_bucket};
use nats_engine::{Bus, BusError};

use crate::job::Failure;

#[derive(Clone)]
pub struct Checkpoints {
    bucket: Bucket,
}

fn broken(error: BusError) -> Failure {
    Failure::Transient(format!("контрольная точка: {error}"))
}

impl Checkpoints {
    pub async fn open(bus: &Bus, bucket: &str) -> Result<Checkpoints, BusError> {
        Ok(Checkpoints {
            bucket: open_bucket(bus, bucket, std::time::Duration::ZERO).await?,
        })
    }

    pub async fn get(&self, job: &str) -> Result<Option<u64>, BusError> {
        let raw = self.bucket.get(job).await?;
        Ok(raw.and_then(|raw| String::from_utf8(raw).ok()?.parse().ok()))
    }

    pub async fn set(&self, job: &str, item: u64) -> Result<(), BusError> {
        self.bucket
            .put(job, item.to_string().as_bytes())
            .await
            .map(|_| ())
    }
}

pub async fn run_checkpointed<F, Fut>(
    job: &str,
    items: &[u64],
    checkpoints: &Checkpoints,
    mut send: F,
) -> Result<usize, Failure>
where
    F: FnMut(String, u64) -> Fut,
    Fut: Future<Output = Result<(), Failure>>,
{
    let passed = checkpoints.get(job).await.map_err(broken)?;
    let mut ordered = items.to_vec();
    ordered.sort_unstable();
    ordered.dedup();
    let mut sent = 0;
    for item in ordered {
        if passed.is_some_and(|passed| item <= passed) {
            continue;
        }
        send(format!("{job}:{item}"), item).await?;
        sent += 1;
        checkpoints.set(job, item).await.map_err(broken)?;
    }
    Ok(sent)
}
