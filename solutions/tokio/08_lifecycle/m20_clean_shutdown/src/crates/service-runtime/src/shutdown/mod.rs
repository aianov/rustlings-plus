use std::time::Duration;

use tokio::time::{Instant, timeout_at};

use crate::token::CancellationToken;
use crate::tracker::TaskTracker;

#[derive(Clone)]
pub struct Stage {
    pub name: &'static str,
    pub token: CancellationToken,
    pub tasks: TaskTracker,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StageReport {
    pub name: &'static str,
    pub aborted: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ShutdownReport {
    pub stages: Vec<StageReport>,
}

impl ShutdownReport {
    pub fn aborted(&self) -> usize {
        self.stages.iter().map(|stage| stage.aborted).sum()
    }

    pub fn order(&self) -> Vec<&'static str> {
        self.stages.iter().map(|stage| stage.name).collect()
    }
}

#[derive(Default)]
pub struct Shutdown {
    root: CancellationToken,
    stages: Vec<Stage>,
}

impl Shutdown {
    pub fn new() -> Shutdown {
        Shutdown::default()
    }

    pub fn stage(&mut self, name: &'static str) -> Stage {
        let stage = Stage {
            name,
            token: self.root.child(),
            tasks: TaskTracker::new(),
        };
        self.stages.push(stage.clone());
        stage
    }

    pub fn token(&self) -> CancellationToken {
        self.root.clone()
    }

    pub async fn close(&self, deadline: Duration) -> ShutdownReport {
        let deadline = Instant::now() + deadline;
        let mut report = ShutdownReport::default();
        for stage in &self.stages {
            stage.token.cancel();
            stage.tasks.close();
            let aborted = match timeout_at(deadline, stage.tasks.wait()).await {
                Ok(()) => 0,
                Err(_) => {
                    let aborted = stage.tasks.abort_all();
                    stage.tasks.wait().await;
                    aborted
                }
            };
            report.stages.push(StageReport {
                name: stage.name,
                aborted,
            });
        }
        self.root.cancel();
        report
    }
}
