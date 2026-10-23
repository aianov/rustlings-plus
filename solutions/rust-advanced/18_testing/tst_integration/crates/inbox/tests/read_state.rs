mod common;

use common::inbox_with;
use inbox::{Inbox, InboxError};

#[test]
fn mark_read_changes_only_that_message() {
    let mut inbox = inbox_with(3);
    inbox.mark_read(2).unwrap();
    assert!(inbox.get(2).unwrap().read);
    assert!(!inbox.get(1).unwrap().read);
    let unread: Vec<u32> = inbox.unread().iter().map(|m| m.id).collect();
    assert_eq!(unread, vec![1, 3]);
}

#[test]
fn unknown_id_is_reported_with_that_id() {
    let mut inbox = inbox_with(1);
    assert_eq!(inbox.mark_read(42), Err(InboxError::NotFound(42)));
}

#[test]
fn clear_read_drops_only_read_messages() {
    let mut inbox = inbox_with(4);
    inbox.mark_read(1).unwrap();
    inbox.mark_read(4).unwrap();
    assert_eq!(inbox.clear_read(), 2);
    assert_eq!(inbox.len(), 2);
    assert!(inbox.get(2).is_some() && inbox.get(3).is_some(), "непрочитанные должны остаться");
}

#[test]
fn search_ignores_case() {
    let mut inbox = Inbox::new();
    inbox.receive("ann", "Купи ХЛЕБ").unwrap();
    inbox.receive("bob", "позвони маме").unwrap();
    assert_eq!(inbox.search("хлеб"), vec![1]);
    assert!(inbox.search("кот").is_empty());
}

#[test]
#[ignore = "долгий: 20 000 сообщений"]
fn many_messages_keep_ids_unique() {
    let inbox = inbox_with(20_000);
    assert_eq!(inbox.len(), 20_000);
    assert_eq!(inbox.get(20_000).unwrap().text, "сообщение 20000");
}
