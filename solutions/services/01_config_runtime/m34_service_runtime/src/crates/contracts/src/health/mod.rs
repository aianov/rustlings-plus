use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveView {
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckView {
    pub name: String,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub millis: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadyView {
    pub ready: bool,
    pub stopping: bool,
    pub checks: Vec<CheckView>,
}

impl ReadyView {
    pub fn failing(&self) -> Vec<&CheckView> {
        self.checks.iter().filter(|check| !check.ok).collect()
    }

    pub fn summary(&self) -> String {
        if self.stopping {
            return "идёт остановка".to_string();
        }
        let failing: Vec<String> = self
            .failing()
            .into_iter()
            .map(|check| match &check.reason {
                Some(reason) => format!("{} ({reason})", check.name),
                None => check.name.clone(),
            })
            .collect();
        if failing.is_empty() {
            "готов".to_string()
        } else {
            failing.join(", ")
        }
    }
}
