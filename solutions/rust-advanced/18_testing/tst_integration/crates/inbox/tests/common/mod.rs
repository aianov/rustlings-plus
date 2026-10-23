#![allow(dead_code)]
use inbox::Inbox;

pub fn inbox_with(count: u32) -> Inbox {
    let mut inbox = Inbox::new();
    for n in 1..=count {
        inbox.receive("ann", &format!("сообщение {n}")).expect("заготовка должна приниматься");
    }
    inbox
}

pub fn text_of_len(chars: usize) -> String {
    "ж".repeat(chars)
}
