// Уведомитель выбирается при запуске и потом живёт у многих владельцев сразу: у обработчиков запросов,
// у фоновых заданий, у потоков рассылки. Для этого подходит `Arc<dyn Notifier + Send + Sync>`: один
// объект, много владельцев, любой реализации. Два варианта: `Mailbox` складывает сообщения по
// пользователям, `Log` пишет всё в общий журнал.
// TODO: Реализуй трейт для обоих, `pick` (по имени вернуть нужную реализацию, или None) и `fan_out`
// (каждое имя обслуживает отдельный поток, дождаться всех).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub trait Notifier {
    fn notify(&self, user: &str, text: &str);
}

#[derive(Default)]
pub struct Mailbox {
    pub boxes: Mutex<HashMap<String, Vec<String>>>,
}

#[derive(Default)]
pub struct Log {
    pub lines: Mutex<Vec<String>>,
}

pub fn pick(kind: &str, mailbox: &Arc<Mailbox>, log: &Arc<Log>) -> Option<Arc<dyn Notifier + Send + Sync>> {
    todo!("mailbox или log, иначе None; ответ делит владение с переданным Arc")
}

pub fn fan_out(notifier: &Arc<dyn Notifier + Send + Sync>, users: &[&str], text: &str) {
    todo!("по потоку на пользователя и дождаться всех")
}

fn main() {
    let mailbox = Arc::new(Mailbox::default());
    let log = Arc::new(Log::default());
    if let Some(n) = pick("log", &mailbox, &log) {
        n.notify("ann", "hi");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn choice_by_name() {
        let mailbox = Arc::new(Mailbox::default());
        let log = Arc::new(Log::default());
        assert!(pick("nope", &mailbox, &log).is_none());
        pick("mailbox", &mailbox, &log).expect("mailbox есть").notify("ann", "hi");
        pick("log", &mailbox, &log).expect("log есть").notify("bob", "yo");
        assert_eq!(mailbox.boxes.lock().expect("ящики отравлены")["ann"], vec!["hi"]);
        assert_eq!(*log.lines.lock().expect("журнал отравлен"), vec!["bob: yo"]);
    }

    #[test]
    fn picked_notifier_shares_ownership() {
        let mailbox = Arc::new(Mailbox::default());
        let log = Arc::new(Log::default());
        let n = pick("mailbox", &mailbox, &log).expect("mailbox есть");
        assert_eq!(Arc::strong_count(&mailbox), 2, "pick не копирует хранилище, а делит владение");
        drop(n);
        assert_eq!(Arc::strong_count(&mailbox), 1);
    }

    #[test]
    fn fan_out_reaches_everyone() {
        let mailbox = Arc::new(Mailbox::default());
        let log = Arc::new(Log::default());
        let n = pick("mailbox", &mailbox, &log).expect("mailbox есть");
        fan_out(&n, &["ann", "bob", "cat"], "news");
        let boxes = mailbox.boxes.lock().expect("ящики отравлены");
        assert_eq!(boxes.len(), 3);
        assert!(boxes.values().all(|v| v == &vec!["news".to_string()]), "каждому по одному сообщению");
    }

    #[test]
    fn fan_out_is_safe_with_a_shared_log() {
        let mailbox = Arc::new(Mailbox::default());
        let log = Arc::new(Log::default());
        let n = pick("log", &mailbox, &log).expect("log есть");
        let users: Vec<String> = (0..20).map(|i| format!("u{i}")).collect();
        let refs: Vec<&str> = users.iter().map(String::as_str).collect();
        fan_out(&n, &refs, "x");
        assert_eq!(log.lines.lock().expect("журнал отравлен").len(), 20, "ни одно сообщение не должно потеряться");
        assert_eq!(Arc::strong_count(&log), 2, "потоки закончились и отпустили свои указатели");
    }
}
