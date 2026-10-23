// Реестр соединений шлюза работает, но через неделю процесс разбух: в памяти сотни тысяч пользователей, хотя онлайн
// несколько тысяч. Мобильные клиенты закрывают приложение, не попрощавшись, и обработчик соединения пропадает,
// не вызвав unregister. А ещё пользователь, закрывший последнее соединение честно, остаётся в списке «в сети».
// Цель: реестр не хранит мёртвое. Отправка, которая убедилась, что получатель закрыт, убирает соединение, а пользователь
// без соединений исчезает из реестра целиком. tracked_users показывает, сколько пользователей реестр хранит.

use std::collections::HashMap;
use tokio::sync::mpsc::UnboundedSender;

fn main() {}

pub type ConnId = u64;

#[derive(Default)]
pub struct Registry {
    next: ConnId,
    users: HashMap<String, HashMap<ConnId, UnboundedSender<String>>>,
}

impl Registry {
    pub fn new() -> Registry {
        Registry::default()
    }

    pub fn register(&mut self, user: &str, tx: UnboundedSender<String>) -> ConnId {
        self.next += 1;
        self.users.entry(user.to_string()).or_default().insert(self.next, tx);
        self.next
    }

    pub fn unregister(&mut self, user: &str, conn: ConnId) -> bool {
        match self.users.get_mut(user) {
            Some(connections) => connections.remove(&conn).is_some(),
            None => false,
        }
    }

    pub fn send_to_user(&self, user: &str, message: &str) -> usize {
        self.users.get(user).map_or(0, |connections| connections.values().filter(|tx| tx.send(message.to_string()).is_ok()).count())
    }

    pub fn online_users(&self) -> Vec<String> {
        let mut users: Vec<String> = self.users.keys().cloned().collect();
        users.sort();
        users
    }

    pub fn tracked_users(&self) -> usize {
        self.users.len()
    }

    pub fn devices(&self, user: &str) -> usize {
        self.users.get(user).map_or(0, HashMap::len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

    fn connect(registry: &mut Registry, user: &str) -> (ConnId, UnboundedReceiver<String>) {
        let (tx, rx) = unbounded_channel();
        (registry.register(user, tx), rx)
    }

    #[test]
    fn the_last_clean_disconnect_removes_the_user() {
        let mut registry = Registry::new();
        let (a, _ra) = connect(&mut registry, "alice");
        let (b, _rb) = connect(&mut registry, "alice");
        registry.unregister("alice", a);
        assert_eq!(registry.online_users(), vec!["alice"], "одно устройство ещё онлайн");
        registry.unregister("alice", b);
        assert!(registry.online_users().is_empty(), "последнее соединение закрыто: пользователь не в сети");
        assert_eq!(registry.tracked_users(), 0, "и записи о нём в памяти нет");
    }

    #[test]
    fn a_vanished_client_is_noticed_by_the_next_send() {
        let mut registry = Registry::new();
        let (_, gone) = connect(&mut registry, "alice");
        let (_, mut alive) = connect(&mut registry, "alice");
        drop(gone);
        assert_eq!(registry.send_to_user("alice", "привет"), 1, "доставлено только живому");
        assert_eq!(registry.devices("alice"), 1, "мёртвое соединение убрано");
        assert_eq!(alive.try_recv().as_deref(), Ok("привет"), "живой получил");
    }

    #[test]
    fn a_user_whose_every_client_vanished_leaves_the_registry() {
        let mut registry = Registry::new();
        for i in 0..1000 {
            let (_, rx) = connect(&mut registry, &format!("user{i}"));
            drop(rx);
        }
        assert_eq!(registry.tracked_users(), 1000, "до отправки о пропаже никто не знает");
        for i in 0..1000 {
            assert_eq!(registry.send_to_user(&format!("user{i}"), "ping"), 0, "доставлять некому");
        }
        assert_eq!(registry.tracked_users(), 0, "после отправок реестр пуст: мёртвое не копится");
        assert!(registry.online_users().is_empty(), "и онлайна нет");
    }

    #[test]
    fn unknown_users_and_ids_are_harmless() {
        let mut registry = Registry::new();
        assert!(!registry.unregister("ghost", 5), "неизвестный пользователь");
        assert_eq!(registry.send_to_user("ghost", "x"), 0, "отправка в никуда");
        let (_, _rx) = connect(&mut registry, "alice");
        assert!(!registry.unregister("alice", 999), "неизвестный номер соединения");
        assert_eq!(registry.tracked_users(), 1, "живой пользователь остался");
    }
}
