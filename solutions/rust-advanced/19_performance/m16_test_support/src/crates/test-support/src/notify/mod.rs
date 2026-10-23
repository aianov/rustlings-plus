use std::sync::{Mutex, MutexGuard, PoisonError};

use social_core::{Notification, Notifier, UserId};

#[derive(Debug, Default)]
pub struct RecordingNotifier {
    calls: Mutex<Vec<(UserId, Notification)>>,
}

impl RecordingNotifier {
    pub fn new() -> RecordingNotifier {
        RecordingNotifier::default()
    }

    pub fn calls(&self) -> Vec<(UserId, Notification)> {
        self.lock().clone()
    }

    pub fn count(&self) -> usize {
        self.lock().len()
    }

    pub fn for_user(&self, user: UserId) -> Vec<Notification> {
        self.lock()
            .iter()
            .filter(|(to, _)| *to == user)
            .map(|(_, notification)| *notification)
            .collect()
    }

    fn lock(&self) -> MutexGuard<'_, Vec<(UserId, Notification)>> {
        self.calls.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Notifier for RecordingNotifier {
    fn notify(&self, user: UserId, notification: Notification) {
        self.lock().push((user, notification));
    }
}
