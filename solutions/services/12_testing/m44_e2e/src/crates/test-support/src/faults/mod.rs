use std::collections::BTreeSet;
use std::sync::{Arc, Mutex, PoisonError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fault {
    Never,
    Always,
    Next(u32),
    Calls(BTreeSet<u64>),
    Every(u64),
}

#[derive(Debug)]
struct State {
    fault: Fault,
    calls: u64,
    failed: u64,
}

#[derive(Debug, Clone)]
pub struct Faults {
    state: Arc<Mutex<State>>,
}

impl Default for Faults {
    fn default() -> Faults {
        Faults::new(Fault::Never)
    }
}

impl Faults {
    pub fn new(fault: Fault) -> Faults {
        Faults {
            state: Arc::new(Mutex::new(State {
                fault,
                calls: 0,
                failed: 0,
            })),
        }
    }

    pub fn calls_failing(calls: &[u64]) -> Faults {
        Faults::new(Fault::Calls(calls.iter().copied().collect()))
    }

    pub fn set(&self, fault: Fault) {
        self.lock().fault = fault;
    }

    pub fn heal(&self) {
        self.set(Fault::Never);
    }

    pub fn calls(&self) -> u64 {
        self.lock().calls
    }

    pub fn failed(&self) -> u64 {
        self.lock().failed
    }

    pub fn hit(&self) -> bool {
        let mut state = self.lock();
        state.calls += 1;
        let call = state.calls;
        let fail = match &mut state.fault {
            Fault::Never => false,
            Fault::Always => true,
            Fault::Next(left) => {
                let fail = *left > 0;
                *left = left.saturating_sub(1);
                fail
            }
            Fault::Calls(numbers) => numbers.contains(&call),
            Fault::Every(every) => *every > 0 && call % *every == 0,
        };
        if fail {
            state.failed += 1;
        }
        fail
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}
