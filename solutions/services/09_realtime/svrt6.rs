use std::collections::HashMap;
use std::sync::{Arc, Mutex};

fn main() {}

pub struct Presence {
    ttl_ms: u64,
    beats: HashMap<String, HashMap<String, u64>>,
}

impl Presence {
    pub fn new(ttl_ms: u64) -> Presence {
        Presence { ttl_ms, beats: HashMap::new() }
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

pub type Shared = Arc<Mutex<Presence>>;

pub struct Gateway {
    id: String,
    presence: Shared,
    next: u64,
    open: HashMap<u64, String>,
}

impl Gateway {
    pub fn new(id: &str, presence: Shared) -> Gateway {
        Gateway { id: id.to_string(), presence, next: 0, open: HashMap::new() }
    }

    fn device(&self, conn: u64) -> String {
        format!("{}:{conn}", self.id)
    }

    pub fn connect(&mut self, now: u64, user: &str) -> u64 {
        self.next += 1;
        self.open.insert(self.next, user.to_string());
        let device = self.device(self.next);
        self.presence.lock().unwrap().heartbeat(user, &device, now);
        self.next
    }

    pub fn disconnect(&mut self, _now: u64, conn: u64) {
        if let Some(user) = self.open.remove(&conn) {
            let device = self.device(conn);
            self.presence.lock().unwrap().leave(&user, &device);
        }
    }

    pub fn tick(&mut self, now: u64) {
        let mut presence = self.presence.lock().unwrap();
        for (conn, user) in &self.open {
            presence.heartbeat(user, &format!("{}:{conn}", self.id), now);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shared(ttl_ms: u64) -> Shared {
        Arc::new(Mutex::new(Presence::new(ttl_ms)))
    }

    fn online(presence: &Shared, user: &str, now: u64) -> bool {
        presence.lock().unwrap().is_online(user, now)
    }

    #[test]
    fn a_connected_user_stays_online_while_the_gateway_ticks() {
        let presence = shared(30_000);
        let mut gateway = Gateway::new("gw-a", presence.clone());
        gateway.connect(0, "alice");
        for second in (10..=120).step_by(10) {
            let now = second * 1_000;
            gateway.tick(now);
            assert!(online(&presence, "alice", now), "в момент {second} с приложение открыто, а пользователь «не в сети»: шлюз не присылает сигналы");
        }
    }

    #[test]
    fn closing_one_device_keeps_the_user_online() {
        let presence = shared(30_000);
        let mut gateway = Gateway::new("gw-a", presence.clone());
        let phone = gateway.connect(0, "alice");
        let laptop = gateway.connect(0, "alice");
        gateway.disconnect(1_000, laptop);
        assert!(online(&presence, "alice", 1_000), "телефон ещё подключён");
        gateway.tick(20_000);
        assert!(online(&presence, "alice", 40_000), "телефон продолжает слать сигналы");
        gateway.disconnect(41_000, phone);
        assert!(!online(&presence, "alice", 41_000), "закрыто последнее устройство: сразу не в сети, ждать ttl незачем");
    }

    #[test]
    fn devices_of_different_gateways_are_different_devices() {
        let presence = shared(30_000);
        let mut a = Gateway::new("gw-a", presence.clone());
        let mut b = Gateway::new("gw-b", presence.clone());
        let on_a = a.connect(0, "alice");
        let on_b = b.connect(0, "alice");
        assert_eq!(on_a, on_b, "у обоих шлюзов первое соединение имеет один и тот же номер, и это нормально");
        a.disconnect(1_000, on_a);
        assert!(online(&presence, "alice", 1_000), "закрылось устройство на шлюзе A, устройство на B живо");
        b.tick(25_000);
        assert!(online(&presence, "alice", 50_000), "B продолжает слать сигналы");
    }

    #[test]
    fn a_crashed_gateway_users_vanish_after_the_ttl_without_anyone_saying_goodbye() {
        let presence = shared(30_000);
        let mut a = Gateway::new("gw-a", presence.clone());
        let mut b = Gateway::new("gw-b", presence.clone());
        a.connect(0, "alice");
        b.connect(0, "bob");
        a.tick(20_000);
        b.tick(20_000);
        drop(a);
        b.tick(40_000);
        assert!(online(&presence, "alice", 49_999), "последний сигнал в 20 с, до 50 с alice ещё считается в сети");
        assert!(!online(&presence, "alice", 50_000), "шлюз упал: через ttl alice не в сети, хотя «вышла» никто не писал");
        assert!(online(&presence, "bob", 50_000), "пользователи живого шлюза не пострадали");
    }

    #[test]
    fn ticking_an_empty_gateway_and_closing_unknown_connections_is_harmless() {
        let presence = shared(10_000);
        let mut gateway = Gateway::new("gw-a", presence.clone());
        gateway.tick(5);
        gateway.disconnect(5, 42);
        assert!(!online(&presence, "nobody", 5), "никого нет");
    }
}
