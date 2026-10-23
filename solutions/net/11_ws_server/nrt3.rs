// Один пользователь сидит в соцсети с телефона и с ноутбука, и личное сообщение должно прийти на оба устройства.
// Реестр из прошлой задачи знает только соединения, а доставлять надо людям. Заведи отображение «пользователь, его устройства»
// и следи, чтобы оно не накапливало мусор: ушёл последний телефон, записи о пользователе в реестре быть не должно.
// TODO: Допиши Hub: connect возвращает гард (при Drop соединение пропадает, а пустой пользователь тоже), send_user доставляет всем устройствам.

#![allow(dead_code)]
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

type UserId = u64;
type ConnId = u64;
type Devices = HashMap<ConnId, mpsc::Sender<String>>;

#[derive(Clone, Default)]
struct Hub {
    users: Arc<Mutex<HashMap<UserId, Devices>>>,
    next: Arc<AtomicU64>,
}

struct Guard {
    hub: Hub,
    user: UserId,
    conn: ConnId,
}

impl Hub {
    fn connect(&self, user: UserId, tx: mpsc::Sender<String>) -> Guard {
        let conn = self.next.fetch_add(1, Ordering::Relaxed) + 1;
        self.users.lock().unwrap().entry(user).or_default().insert(conn, tx);
        Guard { hub: self.clone(), user, conn }
    }

    fn send_user(&self, user: UserId, message: &str) -> usize {
        let senders: Vec<mpsc::Sender<String>> = match self.users.lock().unwrap().get(&user) {
            Some(devices) => devices.values().cloned().collect(),
            None => return 0,
        };
        senders.iter().filter(|tx| tx.try_send(message.to_string()).is_ok()).count()
    }

    fn devices(&self, user: UserId) -> usize {
        self.users.lock().unwrap().get(&user).map_or(0, |d| d.len())
    }

    fn online_users(&self) -> usize {
        self.users.lock().unwrap().len()
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        let mut users = self.hub.users.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(devices) = users.get_mut(&self.user) {
            devices.remove(&self.conn);
            if devices.is_empty() {
                users.remove(&self.user);
            }
        }
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_reaches_every_device() {
        let hub = Hub::default();
        let (phone_tx, mut phone) = mpsc::channel(4);
        let (laptop_tx, mut laptop) = mpsc::channel(4);
        let _g1 = hub.connect(7, phone_tx);
        let _g2 = hub.connect(7, laptop_tx);
        assert_eq!(hub.devices(7), 2, "оба устройства должны быть записаны на одного пользователя");
        assert_eq!(hub.send_user(7, "привет"), 2, "send_user должен вернуть число устройств, куда доставлено");
        assert_eq!(phone.try_recv().as_deref(), Ok("привет"), "телефон не получил сообщение");
        assert_eq!(laptop.try_recv().as_deref(), Ok("привет"), "ноутбук не получил сообщение");
    }

    #[test]
    fn other_users_are_not_touched() {
        let hub = Hub::default();
        let (a_tx, mut a) = mpsc::channel(4);
        let (b_tx, mut b) = mpsc::channel(4);
        let _ga = hub.connect(1, a_tx);
        let _gb = hub.connect(2, b_tx);
        assert_eq!(hub.send_user(1, "только тебе"), 1);
        assert_eq!(a.try_recv().as_deref(), Ok("только тебе"));
        assert!(b.try_recv().is_err(), "чужое личное сообщение попало другому пользователю");
        assert_eq!(hub.send_user(99, "никого нет"), 0, "для неизвестного пользователя доставлено 0");
    }

    #[test]
    fn closing_one_device_keeps_the_other() {
        let hub = Hub::default();
        let (phone_tx, _phone) = mpsc::channel(4);
        let (laptop_tx, mut laptop) = mpsc::channel(4);
        let g1 = hub.connect(7, phone_tx);
        let _g2 = hub.connect(7, laptop_tx);
        drop(g1);
        assert_eq!(hub.devices(7), 1, "ушёл телефон, ноутбук должен остаться");
        assert_eq!(hub.send_user(7, "ещё тут"), 1);
        assert_eq!(laptop.try_recv().as_deref(), Ok("ещё тут"));
    }

    #[test]
    fn last_device_removes_the_user() {
        let hub = Hub::default();
        for round in 0..50 {
            let (tx, _rx) = mpsc::channel(1);
            let g = hub.connect(round, tx);
            assert_eq!(hub.online_users(), 1);
            drop(g);
        }
        assert_eq!(hub.online_users(), 0, "пустые записи пользователей копятся: после ухода последнего устройства запись должна исчезнуть");
    }

    #[test]
    fn a_full_device_does_not_block_the_others() {
        let hub = Hub::default();
        let (stuck_tx, _stuck) = mpsc::channel(1);
        let (fine_tx, mut fine) = mpsc::channel(8);
        let _g1 = hub.connect(5, stuck_tx);
        let _g2 = hub.connect(5, fine_tx);
        assert_eq!(hub.send_user(5, "раз"), 2);
        assert_eq!(hub.send_user(5, "два"), 1, "у зависшего устройства очередь полна: доставлено должно быть только на живое");
        assert_eq!(fine.try_recv().as_deref(), Ok("раз"));
        assert_eq!(fine.try_recv().as_deref(), Ok("два"));
    }

    #[test]
    fn reconnect_after_close_gets_a_fresh_entry() {
        let hub = Hub::default();
        let (tx1, _rx1) = mpsc::channel(4);
        drop(hub.connect(3, tx1));
        let (tx2, mut rx2) = mpsc::channel(4);
        let _g = hub.connect(3, tx2);
        assert_eq!(hub.send_user(3, "снова онлайн"), 1);
        assert_eq!(rx2.try_recv().as_deref(), Ok("снова онлайн"));
    }
}
