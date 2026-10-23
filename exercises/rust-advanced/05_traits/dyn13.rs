// Уведомитель используют по-разному: и конкретным типом, и как `&dyn Notifier`, и как `Box<dyn Notifier>`
// из реестра, и как `Arc<dyn Notifier>` из общего состояния. Функция `broadcast` написана обобщённо, но
// принимает только конкретные типы: передать ей `&dyn Notifier` нельзя («the size for values of type
// dyn Notifier cannot be known»), а `Box<dyn Notifier>` не считается уведомителем, хотя содержит его.
// TODO: Заставь `broadcast` принимать все четыре вида. Подумай, чего не хватает обобщённому параметру
// и что нужно добавить для «умных обёрток», чтобы они сами считались уведомителями.

use std::sync::Arc;

pub trait Notifier {
    fn send(&self, message: &str) -> String;
}

pub struct Email(pub String);
pub struct Sms;

impl Notifier for Email {
    fn send(&self, message: &str) -> String {
        format!("email to {}: {message}", self.0)
    }
}

impl Notifier for Sms {
    fn send(&self, message: &str) -> String {
        format!("sms: {message}")
    }
}

pub fn broadcast<N: Notifier>(notifier: &N, messages: &[&str]) -> Vec<String> {
    messages.iter().map(|m| notifier.send(m)).collect()
}

pub fn demo_arc() -> Vec<String> {
    let shared: Arc<dyn Notifier> = Arc::new(Sms);
    broadcast(&shared, &["a"])
}

fn main() {
    println!("{:?}", broadcast(&Sms, &["hi"]));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concrete_type() {
        assert_eq!(broadcast(&Sms, &["a", "b"]), vec!["sms: a", "sms: b"]);
    }

    #[test]
    fn dyn_reference() {
        let n: &dyn Notifier = &Email("ann".into());
        assert_eq!(broadcast(n, &["hi"]), vec!["email to ann: hi"], "&dyn Notifier передаётся как есть, сам размер неизвестен");
    }

    #[test]
    fn boxed_dyn_is_a_notifier_itself() {
        let n: Box<dyn Notifier> = Box::new(Sms);
        assert_eq!(broadcast(&n, &["x"]), vec!["sms: x"]);
        fn take_by_value<N: Notifier>(n: N) -> String {
            n.send("v")
        }
        assert_eq!(take_by_value(n), "sms: v", "сам Box<dyn Notifier> обязан быть Notifier");
    }

    #[test]
    fn arc_and_plain_reference() {
        assert_eq!(demo_arc(), vec!["sms: a"]);
        let e = Email("bob".into());
        fn take_by_value<N: Notifier>(n: N) -> String {
            n.send("r")
        }
        assert_eq!(take_by_value(&e), "email to bob: r", "ссылка на уведомитель тоже уведомитель");
    }
}
