use std::time::{SystemTime, UNIX_EPOCH};

use crate::cursor::Timestamp;

/// ```
/// use contracts::{Clock, Timestamp};
///
/// struct Frozen(Timestamp);
///
/// impl Clock for Frozen {
///     fn now(&self) -> Timestamp {
///         self.0
///     }
/// }
///
/// let clock: Box<dyn Clock> = Box::new(Frozen(1_700_000_000));
/// assert_eq!(clock.now(), 1_700_000_000);
/// ```
pub trait Clock: Send + Sync {
    fn now(&self) -> Timestamp;
}

/// ```
/// use contracts::{Clock, SystemClock};
///
/// let before = SystemClock.now();
/// assert!(before > 1_700_000_000);
/// assert!(SystemClock.now() >= before);
/// ```
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|since| since.as_secs())
            .unwrap_or_default()
    }
}
