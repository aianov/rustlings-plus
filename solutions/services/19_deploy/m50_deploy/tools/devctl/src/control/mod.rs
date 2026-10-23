use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use healthcheck::{PROBE_TIMEOUT, probe};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tokio::time::{Instant, sleep};

use crate::plan::{Plan, Unit};
use crate::process::{POLL, Stopped, alive, signal};

pub const READY_TIMEOUT: Duration = Duration::from_secs(30);

pub const STOP_GRACE: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Instance {
    pub name: String,
    pub pid: Option<u32>,
    pub alive: bool,
    pub ready: bool,
}

pub struct Devctl {
    state: PathBuf,
    grace: Duration,
    ready_timeout: Duration,
    children: Mutex<HashMap<String, Child>>,
}

async fn ready_now(url: &str) -> bool {
    let url = url.to_string();
    tokio::task::spawn_blocking(move || probe(&url, PROBE_TIMEOUT).is_ok())
        .await
        .unwrap_or(false)
}

impl Devctl {
    pub fn new(state: impl Into<PathBuf>) -> Devctl {
        Devctl {
            state: state.into(),
            grace: STOP_GRACE,
            ready_timeout: READY_TIMEOUT,
            children: Mutex::new(HashMap::new()),
        }
    }

    pub fn grace(self, grace: Duration) -> Devctl {
        Devctl { grace, ..self }
    }

    pub fn ready_within(self, ready_timeout: Duration) -> Devctl {
        Devctl {
            ready_timeout,
            ..self
        }
    }

    fn pid_file(&self, name: &str) -> PathBuf {
        self.state.join(format!("{name}.pid"))
    }

    pub fn log_file(&self, name: &str) -> PathBuf {
        self.state.join("logs").join(format!("{name}.log"))
    }

    pub fn pid(&self, name: &str) -> Option<u32> {
        std::fs::read_to_string(self.pid_file(name))
            .ok()
            .and_then(|text| text.trim().parse().ok())
    }

    pub async fn start(&self, unit: &Unit) -> Result<u32, String> {
        if let Some(pid) = self.pid(&unit.name) {
            if alive(pid).await {
                return Err(format!("{} уже работает (pid {pid})", unit.name));
            }
        }
        std::fs::create_dir_all(self.state.join("logs")).map_err(|error| error.to_string())?;
        let log =
            std::fs::File::create(self.log_file(&unit.name)).map_err(|error| error.to_string())?;
        let err = log.try_clone().map_err(|error| error.to_string())?;
        let child = Command::new(&unit.program)
            .args(&unit.args)
            .envs(
                unit.env
                    .iter()
                    .map(|(key, value)| (key.as_str(), value.as_str())),
            )
            .stdin(Stdio::null())
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(err))
            .spawn()
            .map_err(|error| format!("{} не запустился ({}): {error}", unit.name, unit.program))?;
        let pid = child
            .id()
            .ok_or_else(|| format!("{} сразу завершился", unit.name))?;
        std::fs::write(self.pid_file(&unit.name), pid.to_string())
            .map_err(|error| error.to_string())?;
        self.children.lock().await.insert(unit.name.clone(), child);
        Ok(pid)
    }

    pub async fn wait_ready(&self, unit: &Unit) -> Result<(), String> {
        let Some(url) = &unit.ready else {
            return Ok(());
        };
        let deadline = Instant::now() + self.ready_timeout;
        loop {
            if ready_now(url).await {
                return Ok(());
            }
            let gone = match self.pid(&unit.name) {
                Some(pid) => !self.running(&unit.name, pid).await,
                None => true,
            };
            if gone {
                let log = std::fs::read_to_string(self.log_file(&unit.name)).unwrap_or_default();
                let tail: Vec<&str> = log.lines().rev().take(5).collect();
                return Err(format!(
                    "{} завершился, не став готовым: {}",
                    unit.name,
                    tail.join(" | ")
                ));
            }
            if Instant::now() >= deadline {
                return Err(format!(
                    "{} не готов за {:?}: {url}",
                    unit.name, self.ready_timeout
                ));
            }
            sleep(POLL * 2).await;
        }
    }

    async fn running(&self, name: &str, pid: u32) -> bool {
        let mut children = self.children.lock().await;
        match children.get_mut(name) {
            Some(child) => matches!(child.try_wait(), Ok(None)),
            None => alive(pid).await,
        }
    }

    pub async fn stop(&self, name: &str) -> Stopped {
        let Some(pid) = self.pid(name) else {
            return Stopped::Gone;
        };
        if !self.running(name, pid).await {
            let _ = std::fs::remove_file(self.pid_file(name));
            self.children.lock().await.remove(name);
            return Stopped::Gone;
        }
        let _ = signal(pid, "TERM").await;
        let deadline = Instant::now() + self.grace;
        let mut outcome = Stopped::Terminated;
        while self.running(name, pid).await {
            if Instant::now() >= deadline {
                let _ = signal(pid, "KILL").await;
                outcome = Stopped::Killed;
                let hard = Instant::now() + Duration::from_secs(5);
                while self.running(name, pid).await && Instant::now() < hard {
                    sleep(POLL).await;
                }
                break;
            }
            sleep(POLL).await;
        }
        if let Some(mut child) = self.children.lock().await.remove(name) {
            let _ = child.wait().await;
        }
        let _ = std::fs::remove_file(self.pid_file(name));
        outcome
    }

    pub async fn up(&self, plan: &Plan) -> Result<Vec<String>, String> {
        let order = plan.order()?;
        let mut started = Vec::new();
        for unit in order {
            let launched = match self.start(unit).await {
                Ok(..) => self.wait_ready(unit).await,
                Err(error) => Err(error),
            };
            if let Err(error) = launched {
                started.push(unit.name.clone());
                for name in started.iter().rev() {
                    self.stop(name).await;
                }
                return Err(error);
            }
            started.push(unit.name.clone());
        }
        Ok(started)
    }

    pub async fn down(&self, plan: &Plan) -> Result<Vec<(String, Stopped)>, String> {
        let order = plan.order()?;
        let mut stopped = Vec::new();
        for unit in order.into_iter().rev() {
            let outcome = self.stop(&unit.name).await;
            stopped.push((unit.name.clone(), outcome));
        }
        Ok(stopped)
    }

    pub async fn restart(&self, plan: &Plan, service: &str) -> Result<Vec<String>, String> {
        let order = plan.order()?;
        let instances: Vec<&Unit> = order
            .into_iter()
            .filter(|unit| unit.service == service)
            .collect();
        if instances.is_empty() {
            return Err(format!("сервиса {service} в плане нет"));
        }
        let mut done = Vec::new();
        for unit in instances {
            self.stop(&unit.name).await;
            self.start(unit).await?;
            self.wait_ready(unit).await?;
            done.push(unit.name.clone());
        }
        Ok(done)
    }

    pub async fn status(&self, plan: &Plan) -> Vec<Instance> {
        let mut seen = Vec::new();
        for unit in &plan.units {
            let pid = self.pid(&unit.name);
            let alive = match pid {
                Some(pid) => self.running(&unit.name, pid).await,
                None => false,
            };
            let ready = match &unit.ready {
                Some(url) if alive => ready_now(url).await,
                Some(..) => false,
                None => alive,
            };
            seen.push(Instance {
                name: unit.name.clone(),
                pid,
                alive,
                ready,
            });
        }
        seen
    }
}
