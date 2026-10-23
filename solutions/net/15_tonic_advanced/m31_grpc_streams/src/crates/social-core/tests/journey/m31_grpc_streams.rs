use social_core::{Chats, DomainError, MAX_MESSAGE_LEN, UserId};

#[test]
fn chats_check_validates_without_storing() {
    let (alice, bob) = (UserId(1), UserId(2));
    assert_eq!(
        Chats::check(alice, bob, "  привет  "),
        Ok("привет".to_string()),
        "check возвращает текст, каким его сохранят"
    );
    assert_eq!(
        Chats::check(alice, alice, "я"),
        Err(DomainError::SelfMessage)
    );
    assert_eq!(
        Chats::check(alice, bob, "   "),
        Err(DomainError::EmptyMessage)
    );
    let long = "я".repeat(MAX_MESSAGE_LEN + 1);
    assert_eq!(
        Chats::check(alice, bob, &long),
        Err(DomainError::MessageTooLong)
    );
    let mut chats = Chats::default();
    let _ = Chats::check(alice, bob, "проверка");
    assert!(
        chats.history(alice, bob, 10).is_empty(),
        "проверка ничего не пишет"
    );
    chats
        .send(alice, bob, "запись")
        .expect("send проверяет тем же правилом");
    assert_eq!(chats.history(bob, alice, 10).len(), 1);
}
