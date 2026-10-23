use std::fmt;
use std::future::Future;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use tokio::time::{sleep, timeout};

use crate::background::{Background, Fatal};
use crate::config::Fields;
use crate::health::{Health, StartupError};
use crate::startup::Policy;
use crate::token::CancellationToken;
use crate::trail::Trail;

pub const GRACE: Duration = Duration::from_millis(100);

pub const DRAIN: Duration = Duration::from_secs(2);

pub const GRACE_ENV: &str = "SHUTDOWN_GRACE_MS";

pub const DRAIN_ENV: &str = "SHUTDOWN_DRAIN_MS";

pub const WAIT_ENV: &str = "STARTUP_WAIT_MS";

pub const RETRY_ENV: &str = "STARTUP_RETRY_MS";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timing {
    pub grace: Duration,
    pub drain: Duration,
    pub wait: Option<Policy>,
}

impl Default for Timing {
    fn default() -> Timing {
        Timing {
            grace: GRACE,
            drain: DRAIN,
            wait: None,
        }
    }
}

impl Timing {
    pub fn read(fields: &mut Fields<'_>) -> Timing {
        let standard = Policy::default();
        Timing {
            grace: fields.millis(GRACE_ENV, Duration::from_secs(1)),
            drain: fields.millis(DRAIN_ENV, DRAIN),
            wait: Some(Policy {
                interval: fields.millis(RETRY_ENV, standard.interval),
                total: fields.millis(WAIT_ENV, standard.total),
            }),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    NotReady,
    Closed,
    Drained,
    Abandoned,
    BackgroundStopped,
    Stopped,
}

impl fmt::Display for Step {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Step::NotReady => "не готов: /ready отвечает 503, балансировщик уводит трафик",
            Step::Closed => "приём закрыт, дожидаюсь запросов в полёте",
            Step::Drained => "запросы в полёте завершились",
            Step::Abandoned => "запросы не уложились в срок и оборваны",
            Step::BackgroundStopped => "фоновые задачи остановлены",
            Step::Stopped => "остановлен",
        })
    }
}

enum Early {
    Startup(StartupError),
    Signal,
    Background,
}

#[derive(Debug)]
pub enum Halt<E> {
    Startup(StartupError),
    Serve(E),
    Fatal(Fatal),
}

#[derive(Clone)]
pub struct Lifecycle {
    stopping: CancellationToken,
    closing: CancellationToken,
    background: CancellationToken,
    finished: CancellationToken,
    health: Health,
    grace: Duration,
    drain: Duration,
    trail: Trail,
    steps: Arc<Mutex<Vec<Step>>>,
}

impl Lifecycle {
    pub fn new(signal: &CancellationToken, health: Health, trail: Trail) -> Lifecycle {
        Lifecycle {
            stopping: signal.child(),
            closing: CancellationToken::new(),
            background: CancellationToken::new(),
            finished: CancellationToken::new(),
            health,
            grace: GRACE,
            drain: DRAIN,
            trail,
            steps: Arc::default(),
        }
    }

    pub fn timing(mut self, timing: Timing) -> Lifecycle {
        self.grace = timing.grace;
        self.drain = timing.drain;
        self
    }

    pub fn health(&self) -> &Health {
        &self.health
    }

    pub fn trail(&self) -> &Trail {
        &self.trail
    }

    pub fn stopping(&self) -> CancellationToken {
        self.stopping.clone()
    }

    pub fn closing(&self) -> CancellationToken {
        self.closing.clone()
    }

    pub fn background(&self) -> CancellationToken {
        self.background.clone()
    }

    pub fn finished(&self) -> CancellationToken {
        self.finished.clone()
    }

    pub fn steps(&self) -> Vec<Step> {
        self.steps
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn step(&self, step: Step) {
        self.steps
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(step);
        self.trail.note(&step.to_string());
    }

    async fn wait_ready(&self, policy: Policy) -> Result<(), StartupError> {
        let names = self.health.names();
        if names.is_empty() {
            return Ok(());
        }
        self.trail.note(&format!(
            "жду готовности ({}) не дольше {} мс",
            names.join(", "),
            policy.total.as_millis()
        ));
        let attempts = self.health.wait(&policy).await?;
        self.trail
            .note(&format!("готов к работе с попытки {attempts}"));
        Ok(())
    }

    pub async fn run<E, F>(
        &self,
        wait: Option<Policy>,
        serving: F,
        tasks: Background,
    ) -> Result<(), Halt<E>>
    where
        F: Future<Output = Result<(), E>>,
    {
        tokio::pin!(serving);
        let tasks = tasks.run(self.background.clone());
        tokio::pin!(tasks);
        let mut fatal = None;
        let mut tasks_done = false;
        let waiting = async {
            match wait {
                Some(policy) => self.wait_ready(policy).await,
                None => Ok(()),
            }
        };
        let early = tokio::select! {
            biased;
            () = self.stopping.cancelled() => Some(Early::Signal),
            result = &mut tasks => {
                tasks_done = true;
                fatal = result.err();
                Some(Early::Background)
            }
            waited = waiting => waited.err().map(Early::Startup),
        };
        let started = early.is_none();
        let mut served = None;
        if started {
            tokio::select! {
                () = self.stopping.cancelled() => {}
                result = &mut serving => served = Some(result),
                result = &mut tasks => {
                    tasks_done = true;
                    fatal = result.err();
                }
            }
        }
        self.stopping.cancel();
        self.health.stop();
        if started {
            self.step(Step::NotReady);
        }
        if started && served.is_none() {
            tokio::select! {
                () = sleep(self.grace) => {}
                result = &mut serving => served = Some(result),
            }
        }
        self.closing.cancel();
        if started {
            self.step(Step::Closed);
        }
        let served = match served {
            Some(result) => result,
            None if started => timeout(self.drain, &mut serving).await.unwrap_or_else(|_| {
                self.step(Step::Abandoned);
                Ok(())
            }),
            None => Ok(()),
        };
        if started && !self.steps().contains(&Step::Abandoned) {
            self.step(Step::Drained);
        }
        self.background.cancel();
        if !tasks_done {
            if let Err(error) = tasks.await {
                fatal = Some(error);
            }
        }
        self.step(Step::BackgroundStopped);
        self.finished.cancel();
        self.step(Step::Stopped);
        if let Some(fatal) = fatal {
            return Err(Halt::Fatal(fatal));
        }
        if let Some(Early::Startup(error)) = early {
            return Err(Halt::Startup(error));
        }
        served.map_err(Halt::Serve)
    }
}
