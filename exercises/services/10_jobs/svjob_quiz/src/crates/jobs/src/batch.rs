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
    let _ = (job_id, items, send, checkpoints);
    todo!()
}
