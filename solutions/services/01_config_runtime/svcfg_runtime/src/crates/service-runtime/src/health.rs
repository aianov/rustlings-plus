use futures::future::{BoxFuture, join_all};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

pub trait Check: Send + Sync {
    fn name(&self) -> &str;
    fn run(&self) -> BoxFuture<'_, Result<(), String>>;
}

#[derive(Debug, PartialEq)]
pub struct Report {
    pub ready: bool,
    pub failures: Vec<(String, String)>,
}

pub struct Health {
    checks: Vec<Arc<dyn Check>>,
    shutting_down: AtomicBool,
    check_timeout: Duration,
}

impl Health {
    pub fn new(check_timeout: Duration) -> Health {
        Health { checks: Vec::new(), shutting_down: AtomicBool::new(false), check_timeout }
    }

    pub fn register(&mut self, check: Arc<dyn Check>) {
        self.checks.push(check);
    }

    pub fn begin_shutdown(&self) {
        self.shutting_down.store(true, Ordering::SeqCst);
    }

    pub fn live(&self) -> bool {
        true
    }

    pub async fn ready(&self) -> Report {
        if self.shutting_down.load(Ordering::SeqCst) {
            return Report {
                ready: false,
                failures: vec![("shutdown".to_string(), "идёт остановка".to_string())],
            };
        }
        let results = join_all(self.checks.iter().map(|check| async move {
            match tokio::time::timeout(self.check_timeout, check.run()).await {
                Ok(Ok(())) => None,
                Ok(Err(reason)) => Some((check.name().to_string(), reason)),
                Err(_) => Some((check.name().to_string(), "timeout".to_string())),
            }
        }))
        .await;
        let failures: Vec<_> = results.into_iter().flatten().collect();
        Report { ready: failures.is_empty(), failures }
    }
}
