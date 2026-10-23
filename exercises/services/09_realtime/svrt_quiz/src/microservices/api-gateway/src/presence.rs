use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub struct Presence {
    ttl_ms: u64,
    beats: HashMap<String, HashMap<String, u64>>,
}

pub type Shared = Arc<Mutex<Presence>>;

impl Presence {
    pub fn new(ttl_ms: u64) -> Presence {
        Presence { ttl_ms, beats: HashMap::new() }
    }

    pub fn shared(ttl_ms: u64) -> Shared {
        Arc::new(Mutex::new(Presence::new(ttl_ms)))
    }

    pub fn heartbeat(&mut self, user: &str, device: &str, now: u64) {
        let last = self.beats.entry(user.to_string()).or_default().entry(device.to_string()).or_insert(now);
        *last = (*last).max(now);
    }

    pub fn leave(&mut self, user: &str, device: &str) {
        if let Some(devices) = self.beats.get_mut(user) {
            devices.remove(device);
            if devices.is_empty() {
                self.beats.remove(user);
            }
        }
    }

    pub fn is_online(&self, user: &str, now: u64) -> bool {
        self.beats.get(user).is_some_and(|devices| devices.values().any(|last| now < *last + self.ttl_ms))
    }
}
