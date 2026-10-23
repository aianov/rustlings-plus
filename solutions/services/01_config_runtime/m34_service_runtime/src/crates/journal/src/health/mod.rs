use std::sync::{Arc, Mutex, PoisonError};

#[derive(Debug, Clone, Default)]
pub struct JournalHealth {
    failure: Arc<Mutex<Option<String>>>,
}

impl JournalHealth {
    fn set(&self, failure: Option<String>) {
        *self.failure.lock().unwrap_or_else(PoisonError::into_inner) = failure;
    }

    pub(crate) fn failed(&self, reason: String) {
        self.set(Some(reason));
    }

    pub(crate) fn recovered(&self) {
        self.set(None);
    }

    pub fn check(&self) -> Result<(), String> {
        match self
            .failure
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
        {
            Some(reason) => Err(reason),
            None => Ok(()),
        }
    }
}
