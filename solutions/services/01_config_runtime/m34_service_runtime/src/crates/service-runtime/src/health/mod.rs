use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use tokio::task::JoinSet;
use tokio::time::{Instant, timeout};

use crate::startup::{Policy, wait_for};
use crate::token::CancellationToken;

pub const CHECK_LIMIT: Duration = Duration::from_millis(500);

pub type CheckFuture<'a> = Pin<Box<dyn Future<Output = Result<(), String>> + Send + 'a>>;

pub trait Check: Send + Sync + 'static {
    fn run(&self) -> CheckFuture<'_>;
}

impl<F, Fut> Check for F
where
    F: Fn() -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<(), String>> + Send + 'static,
{
    fn run(&self) -> CheckFuture<'_> {
        Box::pin(self())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Probe {
    pub name: String,
    pub outcome: Result<(), String>,
    pub took: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub ready: bool,
    pub stopping: bool,
    pub probes: Vec<Probe>,
}

impl Report {
    pub fn failing(&self) -> Vec<(&str, &str)> {
        self.probes
            .iter()
            .filter_map(|probe| match &probe.outcome {
                Ok(()) => None,
                Err(reason) => Some((probe.name.as_str(), reason.as_str())),
            })
            .collect()
    }

    pub fn summary(&self) -> String {
        if self.stopping {
            return "идёт остановка".to_string();
        }
        let failing: Vec<String> = self
            .failing()
            .into_iter()
            .map(|(name, reason)| format!("{name}: {reason}"))
            .collect();
        if failing.is_empty() {
            "готов".to_string()
        } else {
            failing.join("; ")
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartupError {
    pub attempts: u32,
    pub limit: Duration,
    pub failing: Vec<(String, String)>,
}

impl fmt::Display for StartupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let failing: Vec<String> = self
            .failing
            .iter()
            .map(|(name, reason)| format!("{name} ({reason})"))
            .collect();
        write!(
            f,
            "за {} мс так и не готовы: {}; попыток: {}",
            self.limit.as_millis(),
            failing.join(", "),
            self.attempts
        )
    }
}

impl Error for StartupError {}

type Registered = Vec<(String, Arc<dyn Check>)>;

struct Inner {
    checks: Mutex<Registered>,
    stopped: CancellationToken,
    limit: Duration,
}

#[derive(Clone)]
pub struct Health {
    inner: Arc<Inner>,
}

impl Default for Health {
    fn default() -> Health {
        Health::new(CHECK_LIMIT)
    }
}

impl fmt::Debug for Health {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Health")
            .field("checks", &self.names())
            .field("stopping", &self.is_stopping())
            .finish()
    }
}

impl Health {
    pub fn new(limit: Duration) -> Health {
        Health {
            inner: Arc::new(Inner {
                checks: Mutex::new(Vec::new()),
                stopped: CancellationToken::new(),
                limit,
            }),
        }
    }

    fn checks(&self) -> Registered {
        self.inner
            .checks
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub fn register(&self, name: &str, check: impl Check) {
        self.inner
            .checks
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((name.to_string(), Arc::new(check)));
    }

    pub fn names(&self) -> Vec<String> {
        self.checks().into_iter().map(|(name, _)| name).collect()
    }

    pub fn stop(&self) {
        self.inner.stopped.cancel();
    }

    pub fn is_stopping(&self) -> bool {
        self.inner.stopped.is_cancelled()
    }

    pub async fn stopped(&self) {
        self.inner.stopped.cancelled().await;
    }

    pub async fn report(&self) -> Report {
        if self.is_stopping() {
            return Report {
                ready: false,
                stopping: true,
                probes: Vec::new(),
            };
        }
        let limit = self.inner.limit;
        let mut running = JoinSet::new();
        let mut names = HashMap::new();
        let checks = self.checks();
        for (index, (name, check)) in checks.iter().cloned().enumerate() {
            let task = running.spawn(async move {
                let started = Instant::now();
                let outcome = match timeout(limit, check.run()).await {
                    Ok(outcome) => outcome,
                    Err(..) => Err(format!("timeout: нет ответа за {} мс", limit.as_millis())),
                };
                (index, outcome, started.elapsed())
            });
            names.insert(task.id(), (index, name));
        }
        let mut probes: Vec<(usize, Probe)> = Vec::with_capacity(checks.len());
        while let Some(joined) = running.join_next_with_id().await {
            let (id, index, outcome, took) = match joined {
                Ok((id, (index, outcome, took))) => (id, index, outcome, took),
                Err(error) => {
                    let index = names.get(&error.id()).map_or(0, |(index, _)| *index);
                    let outcome = Err("проверка упала с паникой".to_string());
                    (error.id(), index, outcome, Duration::ZERO)
                }
            };
            let name = names.remove(&id).map(|(_, name)| name).unwrap_or_default();
            probes.push((
                index,
                Probe {
                    name,
                    outcome,
                    took,
                },
            ));
        }
        probes.sort_by_key(|(index, _)| *index);
        let probes: Vec<Probe> = probes.into_iter().map(|(_, probe)| probe).collect();
        Report {
            ready: probes.iter().all(|probe| probe.outcome.is_ok()),
            stopping: false,
            probes,
        }
    }

    pub async fn wait(&self, policy: &Policy) -> Result<u32, StartupError> {
        let probe = || async {
            let report = self.report().await;
            if report.ready { Ok(()) } else { Err(report) }
        };
        wait_for(policy, probe).await.map_err(|gave_up| {
            let failing = match gave_up.last {
                Some(report) => report
                    .failing()
                    .into_iter()
                    .map(|(name, reason)| (name.to_string(), reason.to_string()))
                    .collect(),
                None => self
                    .names()
                    .into_iter()
                    .map(|name| (name, "проверка не успела ответить".to_string()))
                    .collect(),
            };
            StartupError {
                attempts: gave_up.attempts,
                limit: policy.total,
                failing,
            }
        })
    }
}
