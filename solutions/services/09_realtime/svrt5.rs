use std::collections::HashMap;

fn main() {}

pub struct Presence {
    ttl_ms: u64,
    beats: HashMap<String, HashMap<String, u64>>,
}

impl Presence {
    pub fn new(ttl_ms: u64) -> Presence {
        Presence { ttl_ms, beats: HashMap::new() }
    }

    pub fn entries(&self) -> usize {
        self.beats.values().map(HashMap::len).sum()
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

    pub fn online_devices(&self, user: &str, now: u64) -> Vec<String> {
        let mut alive: Vec<String> = self
            .beats
            .get(user)
            .map(|devices| devices.iter().filter(|(_, last)| now < **last + self.ttl_ms).map(|(device, _)| device.clone()).collect())
            .unwrap_or_default();
        alive.sort();
        alive
    }

    pub fn is_online(&self, user: &str, now: u64) -> bool {
        !self.online_devices(user, now).is_empty()
    }

    pub fn purge(&mut self, now: u64) -> usize {
        let before = self.entries();
        let ttl = self.ttl_ms;
        for devices in self.beats.values_mut() {
            devices.retain(|_, last| now < *last + ttl);
        }
        self.beats.retain(|_, devices| !devices.is_empty());
        before - self.entries()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_device_is_alive_until_exactly_ttl_after_its_last_beat() {
        let mut presence = Presence::new(30_000);
        presence.heartbeat("alice", "phone", 1_000);
        assert!(presence.is_online("alice", 1_000), "в момент сигнала жив");
        assert!(presence.is_online("alice", 30_999), "за миллисекунду до границы ещё жив");
        assert!(!presence.is_online("alice", 31_000), "на границе now = last + ttl уже не жив");
        assert!(!presence.is_online("bob", 1_000), "неизвестный пользователь не в сети");
    }

    #[test]
    fn a_fresh_beat_extends_life_and_a_stale_one_does_not_shorten_it() {
        let mut presence = Presence::new(10_000);
        presence.heartbeat("alice", "phone", 5_000);
        presence.heartbeat("alice", "phone", 12_000);
        assert!(presence.is_online("alice", 21_999), "новый сигнал продлил жизнь до 22 000");
        presence.heartbeat("alice", "phone", 6_000);
        assert!(presence.is_online("alice", 21_999), "запоздавший старый сигнал (пришёл в обратном порядке) жизнь не сокращает");
        assert!(!presence.is_online("alice", 22_000), "а дальше жизнь закончилась");
    }

    #[test]
    fn the_user_is_online_while_any_device_is_alive() {
        let mut presence = Presence::new(10_000);
        presence.heartbeat("alice", "phone", 0);
        presence.heartbeat("alice", "laptop", 8_000);
        assert_eq!(presence.online_devices("alice", 9_000), vec!["laptop", "phone"], "оба устройства живы, по алфавиту");
        assert_eq!(presence.online_devices("alice", 12_000), vec!["laptop"], "телефон замолчал, ноутбук жив");
        assert!(presence.is_online("alice", 12_000), "значит, пользователь в сети");
        assert!(!presence.is_online("alice", 18_000), "замолчали оба: не в сети");
    }

    #[test]
    fn leave_removes_only_that_device_and_other_users_are_untouched() {
        let mut presence = Presence::new(10_000);
        presence.heartbeat("alice", "phone", 0);
        presence.heartbeat("alice", "laptop", 0);
        presence.heartbeat("bob", "phone", 0);
        presence.leave("alice", "phone");
        assert_eq!(presence.online_devices("alice", 1), vec!["laptop"], "ноутбук остался");
        assert!(presence.is_online("bob", 1), "у bob устройство с тем же названием не затронуто");
        presence.leave("alice", "laptop");
        presence.leave("alice", "laptop");
        assert!(!presence.is_online("alice", 1), "ушли оба устройства: не в сети сразу, не дожидаясь ttl");
    }

    #[test]
    fn purge_drops_only_the_expired_and_empties_the_memory() {
        let mut presence = Presence::new(10_000);
        for i in 0..100 {
            presence.heartbeat(&format!("user{i}"), "phone", 0);
        }
        presence.heartbeat("alice", "phone", 0);
        presence.heartbeat("alice", "laptop", 9_000);
        assert_eq!(presence.entries(), 102, "записей по числу устройств");
        assert_eq!(presence.purge(10_000), 101, "протухли 100 чужих и телефон alice");
        assert_eq!(presence.entries(), 1, "остался ноутбук alice");
        assert!(presence.is_online("alice", 10_000), "alice по-прежнему в сети");
        assert_eq!(presence.purge(10_000), 0, "второй раз чистить нечего");
        assert_eq!(presence.purge(19_000), 1, "через ttl ноутбук тоже уходит");
        assert_eq!(presence.entries(), 0, "память пуста");
    }
}
