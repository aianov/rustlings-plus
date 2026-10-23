use std::collections::HashMap;

#[derive(Default)]
pub struct Checkpoints {
    values: HashMap<String, String>,
}

impl Checkpoints {
    pub fn get(&self, job_id: &str) -> Option<&str> {
        self.values.get(job_id).map(String::as_str)
    }

    pub fn set(&mut self, job_id: &str, value: &str) {
        self.values.insert(job_id.to_string(), value.to_string());
    }
}

pub fn run_checkpointed(
    job_id: &str,
    items: &[String],
    send: &mut impl FnMut(&str, &str) -> Result<(), String>,
    checkpoints: &mut Checkpoints,
) -> Result<usize, String> {
    let last = checkpoints.get(job_id).map(str::to_string);
    let mut done = 0;
    for item in items {
        if last.as_deref().is_some_and(|last| item.as_str() <= last) {
            continue;
        }
        send(&format!("{job_id}:{item}"), item)?;
        checkpoints.set(job_id, item);
        done += 1;
    }
    Ok(done)
}
