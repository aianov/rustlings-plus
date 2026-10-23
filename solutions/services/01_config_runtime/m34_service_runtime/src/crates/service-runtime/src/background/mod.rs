use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::pin::Pin;

use tokio::task::JoinSet;

use crate::supervisor::{Outcome, Restart, supervise};
use crate::token::CancellationToken;
use crate::trail::Trail;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Criticality {
    Critical,
    Optional,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fatal {
    pub name: String,
    pub attempts: u32,
}

impl fmt::Display for Fatal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "критичная фоновая задача {} упала {} раз подряд",
            self.name, self.attempts
        )
    }
}

impl Error for Fatal {}

type Job = Pin<Box<dyn Future<Output = Outcome> + Send>>;

type Start = Box<dyn FnOnce(CancellationToken) -> Job + Send>;

pub struct Background {
    jobs: Vec<(String, Criticality, Start)>,
    trail: Trail,
}

impl Background {
    pub fn new(trail: Trail) -> Background {
        Background {
            jobs: Vec::new(),
            trail,
        }
    }

    pub fn spawn<F, Fut>(
        &mut self,
        name: impl Into<String>,
        criticality: Criticality,
        policy: Restart,
        make: F,
    ) where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let start: Start = Box::new(move |token| Box::pin(supervise(make, token, policy)));
        self.jobs.push((name.into(), criticality, start));
    }

    pub fn names(&self) -> Vec<&str> {
        self.jobs.iter().map(|(name, ..)| name.as_str()).collect()
    }

    pub fn len(&self) -> usize {
        self.jobs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.jobs.is_empty()
    }

    pub async fn run(self, token: CancellationToken) -> Result<(), Fatal> {
        let mut running = JoinSet::new();
        let mut known = HashMap::new();
        for (name, criticality, start) in self.jobs {
            let task = running.spawn(start(token.clone()));
            known.insert(task.id(), (name, criticality));
        }
        loop {
            let joined = tokio::select! {
                () = token.cancelled() => {
                    while running.join_next().await.is_some() {}
                    return Ok(());
                }
                joined = running.join_next_with_id(), if !running.is_empty() => joined,
            };
            let (id, attempts) = match joined {
                Some(Ok((id, Outcome::GaveUp { attempts }))) => (id, attempts),
                Some(Ok((id, Outcome::Finished { .. }))) => {
                    if let Some((name, _)) = known.get(&id) {
                        self.trail
                            .note(&format!("фоновая задача {name} завершилась"));
                    }
                    continue;
                }
                Some(Ok((_, Outcome::Cancelled { .. }))) | None => continue,
                Some(Err(error)) => (error.id(), 1),
            };
            let Some((name, criticality)) = known.remove(&id) else {
                continue;
            };
            if criticality == Criticality::Critical {
                running.abort_all();
                while running.join_next().await.is_some() {}
                return Err(Fatal { name, attempts });
            }
            self.trail.note(&format!(
                "фоновая задача {name} упала {attempts} раз подряд и выключена, сервис работает без неё"
            ));
        }
    }
}
