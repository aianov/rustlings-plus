use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use contracts::{Push, UserId, encode};
use tokio::sync::mpsc;

use super::outbox::{Out, Outbox, lock};

pub type ConnId = u64;

#[derive(Clone)]
struct Seat {
    outbox: Outbox,
    session: String,
}

type Devices = HashMap<UserId, HashMap<ConnId, Seat>>;

#[derive(Default)]
pub struct Registry {
    devices: Mutex<Devices>,
    next: AtomicU64,
    kicked: Arc<AtomicU64>,
}

pub struct Registration {
    registry: Arc<Registry>,
    user: UserId,
    conn: ConnId,
}

impl Registry {
    fn lock(&self) -> MutexGuard<'_, Devices> {
        lock(&self.devices)
    }

    pub fn outbox(&self, capacity: usize) -> (Outbox, mpsc::Receiver<Out>) {
        Outbox::counted(capacity, Arc::clone(&self.kicked))
    }

    pub fn register(self: &Arc<Registry>, user: UserId, outbox: Outbox) -> Registration {
        self.enter(user, "", outbox)
    }

    pub fn enter(
        self: &Arc<Registry>,
        user: UserId,
        session: &str,
        outbox: Outbox,
    ) -> Registration {
        let conn = self.next.fetch_add(1, Ordering::Relaxed) + 1;
        let seat = Seat {
            outbox,
            session: session.to_string(),
        };
        self.lock().entry(user).or_default().insert(conn, seat);
        Registration {
            registry: Arc::clone(self),
            user,
            conn,
        }
    }

    pub fn devices(&self, user: UserId) -> usize {
        self.lock().get(&user).map_or(0, HashMap::len)
    }

    pub fn users(&self) -> usize {
        self.lock().len()
    }

    pub fn connections(&self) -> usize {
        self.lock().values().map(HashMap::len).sum()
    }

    pub fn kicked(&self) -> u64 {
        self.kicked.load(Ordering::Relaxed)
    }

    pub fn seats(&self) -> Vec<(UserId, ConnId)> {
        let devices = self.lock();
        devices
            .iter()
            .flat_map(|(user, own)| own.keys().map(|conn| (*user, *conn)))
            .collect()
    }

    fn targets(&self, user: UserId) -> Vec<Outbox> {
        let mut devices = self.lock();
        let Some(own) = devices.get_mut(&user) else {
            return Vec::new();
        };
        own.retain(|_, seat| !seat.outbox.is_gone());
        let targets = own.values().map(|seat| seat.outbox.clone()).collect();
        if own.is_empty() {
            devices.remove(&user);
        }
        targets
    }

    pub fn deliver(&self, user: UserId, push: &Push) -> usize {
        let Ok(text) = encode(push) else {
            return 0;
        };
        self.targets(user)
            .iter()
            .filter(|outbox| outbox.send(text.clone()))
            .count()
    }

    pub fn deliver_stored(&self, user: UserId, seq: u64, push: &Push) -> usize {
        let Ok(text) = encode(push) else {
            return 0;
        };
        self.targets(user)
            .iter()
            .filter(|outbox| outbox.stored(seq, text.clone()))
            .count()
    }

    pub fn revoke(&self, sessions: &HashSet<String>) -> usize {
        let devices = self.lock();
        let doomed: Vec<Outbox> = devices
            .values()
            .flat_map(HashMap::values)
            .filter(|seat| !seat.session.is_empty() && sessions.contains(&seat.session))
            .map(|seat| seat.outbox.clone())
            .collect();
        drop(devices);
        for outbox in &doomed {
            outbox.revoke();
        }
        doomed.len()
    }

    fn leave(&self, user: UserId, conn: ConnId) {
        let mut devices = self.lock();
        if let Some(own) = devices.get_mut(&user) {
            own.remove(&conn);
            if own.is_empty() {
                devices.remove(&user);
            }
        }
    }
}

impl Registration {
    pub fn user(&self) -> UserId {
        self.user
    }

    pub fn conn(&self) -> ConnId {
        self.conn
    }
}

impl Drop for Registration {
    fn drop(&mut self) {
        self.registry.leave(self.user, self.conn);
    }
}
