use std::sync::mpsc::Receiver;

use crate::error::JobError;

pub struct JobHandle<T> {
    result: Receiver<T>,
}

impl<T> JobHandle<T> {
    pub(crate) fn new(result: Receiver<T>) -> JobHandle<T> {
        JobHandle { result }
    }

    pub fn join(self) -> Result<T, JobError> {
        self.result.recv().map_err(|_| JobError::Panicked)
    }
}
