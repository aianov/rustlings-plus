use std::fmt;
use std::future::Future;
use std::pin::Pin;

use nats_engine::js::{Bucket, open_bucket};
use nats_engine::{Bus, BusError};
use serde::{Deserialize, Serialize};

use crate::job::Failure;

pub const COMPENSATE_ATTEMPTS: usize = 3;

pub type StepFuture<'a> = Pin<Box<dyn Future<Output = Result<(), Failure>> + Send + 'a>>;

pub trait Step: Send + Sync {
    fn name(&self) -> &str;
    fn forward_only(&self) -> bool;
    fn run(&self) -> StepFuture<'_>;
    fn compensate(&self) -> StepFuture<'_>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "entry", content = "step", rename_all = "snake_case")]
pub enum SagaStep {
    Started(String),
    Done(String),
    Aborted(String),
    Compensated(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SagaError {
    Retry(Failure),
    Store(String),
    RolledBack {
        failed: String,
        reason: String,
        stuck: Vec<String>,
    },
}

impl fmt::Display for SagaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SagaError::Retry(failure) => write!(f, "сага продолжится позже: {failure}"),
            SagaError::Store(reason) => write!(f, "журнал саги недоступен: {reason}"),
            SagaError::RolledBack {
                failed,
                reason,
                stuck,
            } if stuck.is_empty() => {
                write!(f, "шаг {failed} не удался ({reason}), сделанное откачено")
            }
            SagaError::RolledBack {
                failed,
                reason,
                stuck,
            } => write!(
                f,
                "шаг {failed} не удался ({reason}), откатить не вышло: {}",
                stuck.join(", ")
            ),
        }
    }
}

impl std::error::Error for SagaError {}

#[derive(Clone)]
pub struct SagaLog {
    bucket: Bucket,
}

fn store(error: BusError) -> SagaError {
    SagaError::Store(error.to_string())
}

impl SagaLog {
    pub async fn open(bus: &Bus, bucket: &str) -> Result<SagaLog, BusError> {
        Ok(SagaLog {
            bucket: open_bucket(bus, bucket, std::time::Duration::ZERO).await?,
        })
    }

    pub async fn read(&self, saga: &str) -> Result<Vec<SagaStep>, BusError> {
        match self.bucket.get(saga).await? {
            Some(raw) => {
                serde_json::from_slice(&raw).map_err(|error| BusError::Decode(error.to_string()))
            }
            None => Ok(Vec::new()),
        }
    }

    async fn append(
        &self,
        saga: &str,
        entries: &mut Vec<SagaStep>,
        entry: SagaStep,
    ) -> Result<(), SagaError> {
        entries.push(entry);
        let raw =
            serde_json::to_vec(entries).map_err(|error| SagaError::Store(error.to_string()))?;
        self.bucket.put(saga, &raw).await.map(|_| ()).map_err(store)
    }
}

pub struct Saga<'a> {
    id: String,
    steps: Vec<Box<dyn Step + 'a>>,
    log: &'a SagaLog,
}

fn has(entries: &[SagaStep], wanted: impl Fn(&SagaStep) -> bool) -> bool {
    entries.iter().any(wanted)
}

impl<'a> Saga<'a> {
    pub fn new(id: &str, log: &'a SagaLog) -> Saga<'a> {
        Saga {
            id: id.to_string(),
            steps: Vec::new(),
            log,
        }
    }

    pub fn step(mut self, step: impl Step + 'a) -> Saga<'a> {
        self.steps.push(Box::new(step));
        self
    }

    pub async fn run(&self) -> Result<(), SagaError> {
        let mut entries = self.log.read(&self.id).await.map_err(store)?;
        if let Some(reason) = entries.iter().find_map(|entry| match entry {
            SagaStep::Aborted(reason) => Some(reason.clone()),
            SagaStep::Started(..) | SagaStep::Done(..) | SagaStep::Compensated(..) => None,
        }) {
            return self
                .roll_back(&mut entries, String::from("после перезапуска"), reason)
                .await;
        }
        let mut forward = false;
        for step in &self.steps {
            let name = step.name().to_string();
            forward |= step.forward_only();
            if has(&entries, |entry| *entry == SagaStep::Done(name.clone())) {
                continue;
            }
            if !has(&entries, |entry| *entry == SagaStep::Started(name.clone())) {
                self.log
                    .append(&self.id, &mut entries, SagaStep::Started(name.clone()))
                    .await?;
            }
            match step.run().await {
                Ok(()) => {
                    self.log
                        .append(&self.id, &mut entries, SagaStep::Done(name))
                        .await?
                }
                Err(failure) if forward => return Err(SagaError::Retry(failure)),
                Err(failure) => {
                    let reason = failure.to_string();
                    self.log
                        .append(&self.id, &mut entries, SagaStep::Aborted(reason.clone()))
                        .await?;
                    return self.roll_back(&mut entries, name, reason).await;
                }
            }
        }
        Ok(())
    }

    async fn roll_back(
        &self,
        entries: &mut Vec<SagaStep>,
        failed: String,
        reason: String,
    ) -> Result<(), SagaError> {
        let mut stuck = Vec::new();
        let started: Vec<String> = entries
            .iter()
            .filter_map(|entry| match entry {
                SagaStep::Started(name) => Some(name.clone()),
                SagaStep::Done(..) | SagaStep::Aborted(..) | SagaStep::Compensated(..) => None,
            })
            .collect();
        for name in started.iter().rev() {
            let done = has(entries, |entry| *entry == SagaStep::Done(name.clone()));
            let compensated = has(entries, |entry| {
                *entry == SagaStep::Compensated(name.clone())
            });
            if compensated || (*name == failed && !done) {
                continue;
            }
            let Some(step) = self.steps.iter().find(|step| step.name() == name) else {
                continue;
            };
            let mut undone = false;
            for _ in 0..COMPENSATE_ATTEMPTS {
                if step.compensate().await.is_ok() {
                    undone = true;
                    break;
                }
            }
            if undone {
                self.log
                    .append(&self.id, entries, SagaStep::Compensated(name.clone()))
                    .await?;
            } else {
                stuck.push(name.clone());
            }
        }
        Err(SagaError::RolledBack {
            failed,
            reason,
            stuck,
        })
    }
}
