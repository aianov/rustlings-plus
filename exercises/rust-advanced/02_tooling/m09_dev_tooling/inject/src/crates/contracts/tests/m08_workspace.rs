use std::borrow::Cow;
use std::collections::HashSet;

use contracts::{NameError, PostId, UserId, Username, normalize_name};

#[test]
fn ids_are_plain_values() {
    let first = UserId(1);
    let copy = first;
    let users: HashSet<UserId> = [first, copy, UserId(2)].into_iter().collect();
    assert_eq!(
        users.len(),
        2,
        "UserId копируется и годится в ключи (Hash, Eq)"
    );
    let posts: HashSet<PostId> = [PostId(1), PostId(1)].into_iter().collect();
    assert_eq!(posts.len(), 1, "PostId годится в ключи");
}

#[test]
fn username_turns_back_into_string() {
    let name = Username::try_from("  Bob ").expect("нормальный ник проходит проверку");
    assert_eq!(
        String::from(name),
        "bob",
        "From<Username> для String отдаёт нормализованный ник"
    );
}

#[test]
fn normalization_lives_in_contracts() {
    assert!(
        matches!(normalize_name(" alice "), Cow::Borrowed("alice")),
        "normalize_name переехал в contracts без изменений"
    );
    assert!(
        matches!(normalize_name("ALICE"), Cow::Owned(ref name) if name == "alice"),
        "заглавные дают Cow::Owned"
    );
}

#[test]
fn name_errors_read_like_sentences() {
    assert_eq!(
        NameError::Empty.to_string(),
        "ник не может быть пустым",
        "Display для NameError::Empty"
    );
    assert_eq!(
        NameError::TooLong.to_string(),
        "ник длиннее 20 символов",
        "Display для NameError::TooLong"
    );
}
