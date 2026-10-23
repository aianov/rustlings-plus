pub trait Notifier {
    fn send(&self, to: u32, text: &str) -> Result<(), SendError>;
}

#[derive(Debug, PartialEq)]
pub struct SendError;

impl<T: Notifier + ?Sized> Notifier for &T {
    fn send(&self, to: u32, text: &str) -> Result<(), SendError> {
        (**self).send(to, text)
    }
}

use std::sync::Mutex;

pub struct MockNotifier {
    log: Mutex<Vec<(u32, String)>>,
    fail: bool,
}

impl MockNotifier {
    pub fn new() -> MockNotifier {
        MockNotifier { log: Mutex::new(Vec::new()), fail: false }
    }

    pub fn failing() -> MockNotifier {
        MockNotifier { log: Mutex::new(Vec::new()), fail: true }
    }

    pub fn calls(&self) -> Vec<(u32, String)> {
        self.log.lock().expect("журнал отравлен").clone()
    }
}

impl Default for MockNotifier {
    fn default() -> Self {
        MockNotifier::new()
    }
}

impl Notifier for MockNotifier {
    fn send(&self, to: u32, text: &str) -> Result<(), SendError> {
        self.log.lock().expect("журнал отравлен").push((to, text.to_string()));
        if self.fail { Err(SendError) } else { Ok(()) }
    }
}

#[derive(Debug, PartialEq)]
pub enum RegisterError {
    EmptyName,
    NotifyFailed,
}

pub struct Welcome<N: Notifier> {
    notifier: N,
}

impl<N: Notifier> Welcome<N> {
    pub fn new(notifier: N) -> Self {
        Welcome { notifier }
    }

    pub fn register(&self, id: u32, name: &str) -> Result<(), RegisterError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(RegisterError::EmptyName);
        }
        self.notifier.send(id, &format!("Привет, {name}!")).map_err(|_| RegisterError::NotifyFailed)
    }
}

fn main() {
    let mock = MockNotifier::new();
    let _ = Welcome::new(&mock).register(1, "Аня");
    println!("{:?}", mock.calls());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn welcome_is_sent_once_with_the_name() {
        let mock = MockNotifier::new();
        let welcome = Welcome::new(&mock);
        assert_eq!(welcome.register(7, "  Аня "), Ok(()));
        assert_eq!(mock.calls(), vec![(7, "Привет, Аня!".to_string())], "одно письмо адресату 7 с очищенным именем");
    }

    #[test]
    fn invalid_registration_sends_nothing() {
        let mock = MockNotifier::new();
        let welcome = Welcome::new(&mock);
        assert_eq!(welcome.register(1, "   "), Err(RegisterError::EmptyName));
        assert!(mock.calls().is_empty(), "при ошибке проверки никакого письма быть не должно");
    }

    #[test]
    fn calls_keep_their_order() {
        let mock = MockNotifier::new();
        let welcome = Welcome::new(&mock);
        welcome.register(1, "a").unwrap();
        welcome.register(2, "b").unwrap();
        welcome.register(3, "c").unwrap();
        let to: Vec<u32> = mock.calls().iter().map(|(to, _)| *to).collect();
        assert_eq!(to, vec![1, 2, 3], "журнал хранит вызовы в порядке отправки");
    }

    #[test]
    fn failing_notifier_is_reported_and_still_logged() {
        let mock = MockNotifier::failing();
        let welcome = Welcome::new(&mock);
        assert_eq!(welcome.register(4, "Ян"), Err(RegisterError::NotifyFailed));
        assert_eq!(mock.calls().len(), 1, "попытка отправки была, её надо видеть в журнале даже при ошибке");
    }

    #[test]
    fn two_mocks_have_separate_journals() {
        let (first, second) = (MockNotifier::new(), MockNotifier::new());
        Welcome::new(&first).register(1, "a").unwrap();
        assert_eq!(first.calls().len(), 1);
        assert!(second.calls().is_empty(), "у каждого мока свой журнал");
    }
}
