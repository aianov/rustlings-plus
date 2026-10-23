use std::time::Duration;

use tokio::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stopped {
    Gone,
    Terminated,
    Killed,
}

pub async fn signal(pid: u32, name: &str) -> bool {
    Command::new("kill")
        .arg(format!("-{name}"))
        .arg(pid.to_string())
        .output()
        .await
        .is_ok_and(|output| output.status.success())
}

pub async fn alive(pid: u32) -> bool {
    signal(pid, "0").await
}

pub const POLL: Duration = Duration::from_millis(50);
