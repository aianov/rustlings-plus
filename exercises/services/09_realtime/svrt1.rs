// Пользователь заходит в приложение с телефона и с ноутбука, и у него два соединения. Новое сообщение должно прийти на
// оба, а шлюзу нужно знать, кто сейчас онлайн. Цель: Registry хранит соединения по пользователям. register отдаёт
// номер соединения (номера не повторяются ни у одного пользователя), unregister отключает одно соединение и говорит,
// было ли оно, send_to_user отправляет сообщение всем соединениям пользователя и возвращает, скольким дошло, devices
// считает соединения, online_users перечисляет тех, у кого есть хотя бы одно соединение, по алфавиту.

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
        let _ = (user, tx);
        todo!()
    }

    pub fn unregister(&mut self, user: &str, conn: ConnId) -> bool {
        let _ = (user, conn);
        todo!()
    }

    pub fn send_to_user(&self, user: &str, message: &str) -> usize {
        let _ = (user, message);
        todo!()
    }

    pub fn devices(&self, user: &str) -> usize {
        let _ = user;
        todo!()
    }

    pub fn online_users(&self) -> Vec<String> {
        todo!()
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

    fn drain(rx: &mut UnboundedReceiver<String>) -> Vec<String> {
        let mut all = Vec::new();
        while let Ok(message) = rx.try_recv() {
            all.push(message);
        }
        all
    }

    #[test]
    fn a_message_reaches_every_device_of_the_user_and_nobody_else() {
        let mut registry = Registry::new();
        let (_, mut phone) = connect(&mut registry, "alice");
        let (_, mut laptop) = connect(&mut registry, "alice");
        let (_, mut other) = connect(&mut registry, "bob");
        assert_eq!(registry.send_to_user("alice", "привет"), 2, "доставлено на два устройства");
        assert_eq!(drain(&mut phone), vec!["привет"], "телефон получил");
        assert_eq!(drain(&mut laptop), vec!["привет"], "ноутбук получил");
        assert!(drain(&mut other).is_empty(), "чужое соединение не получает ничего");
    }

    #[test]
    fn connection_ids_never_repeat_even_across_users() {
        let mut registry = Registry::new();
        let (a, _ra) = connect(&mut registry, "alice");
        let (b, _rb) = connect(&mut registry, "bob");
        let (c, _rc) = connect(&mut registry, "alice");
        assert!(a != b && b != c && a != c, "номера {a}, {b}, {c} должны быть разными: иначе unregister закроет чужое соединение");
    }

    #[test]
    fn unregister_removes_exactly_one_connection() {
        let mut registry = Registry::new();
        let (first, mut phone) = connect(&mut registry, "alice");
        let (_, mut laptop) = connect(&mut registry, "alice");
        assert!(registry.unregister("alice", first), "соединение было");
        assert!(!registry.unregister("alice", first), "второй раз его уже нет");
        assert!(!registry.unregister("nobody", 99), "и неизвестного пользователя нет");
        assert_eq!(registry.devices("alice"), 1, "осталось одно");
        assert_eq!(registry.send_to_user("alice", "ещё"), 1, "доставка идёт только на оставшееся");
        assert!(drain(&mut phone).is_empty() && drain(&mut laptop) == vec!["ещё"], "телефон отключён, ноутбук получил");
    }

    #[test]
    fn online_means_at_least_one_connection() {
        let mut registry = Registry::new();
        assert!(registry.online_users().is_empty(), "в начале никого нет");
        let (zed, _r1) = connect(&mut registry, "zed");
        let (_, _r2) = connect(&mut registry, "alice");
        let (_, _r3) = connect(&mut registry, "alice");
        assert_eq!(registry.online_users(), vec!["alice", "zed"], "по алфавиту, без повторов");
        registry.unregister("zed", zed);
        assert_eq!(registry.online_users(), vec!["alice"], "у zed соединений не осталось, значит он не в сети");
        assert_eq!(registry.send_to_user("zed", "эй"), 0, "офлайн-пользователю доставлять некуда");
    }

    #[test]
    fn messages_keep_their_order_on_each_connection() {
        let mut registry = Registry::new();
        let (_, mut rx) = connect(&mut registry, "alice");
        for i in 0..20 {
            registry.send_to_user("alice", &format!("m{i}"));
        }
        let expected: Vec<String> = (0..20).map(|i| format!("m{i}")).collect();
        assert_eq!(drain(&mut rx), expected, "порядок не меняется");
    }
}
