mod common;

use common::{inbox_with, text_of_len};
use inbox::{Inbox, InboxError, MAX_LEN};

#[test]
fn ids_start_at_one_and_grow_by_one() {
    let mut inbox = Inbox::new();
    assert_eq!(inbox.receive("ann", "раз"), Ok(1));
    assert_eq!(inbox.receive("bob", "два"), Ok(2));
    assert_eq!(inbox.len(), 2);
}

#[test]
fn blank_text_is_rejected() {
    let mut inbox = Inbox::new();
    assert_eq!(inbox.receive("ann", "   \n\t "), Err(InboxError::Empty));
    assert!(inbox.is_empty(), "отвергнутое сообщение не должно попасть в ящик");
}

#[test]
fn whitespace_is_collapsed() {
    let mut inbox = Inbox::new();
    let id = inbox.receive("ann", "  привет,   как\tдела ").unwrap();
    assert_eq!(inbox.get(id).unwrap().text, "привет, как дела");
}

#[test]
fn limit_counts_characters_not_bytes() {
    let mut inbox = Inbox::new();
    assert!(inbox.receive("ann", &text_of_len(MAX_LEN)).is_ok(), "ровно MAX_LEN символов допустимо, даже если байт вдвое больше");
    assert_eq!(inbox.receive("ann", &text_of_len(MAX_LEN + 1)), Err(InboxError::TooLong(MAX_LEN + 1)));
}

#[test]
fn sender_is_kept() {
    let inbox = inbox_with(1);
    assert_eq!(inbox.get(1).unwrap().from, "ann");
}
