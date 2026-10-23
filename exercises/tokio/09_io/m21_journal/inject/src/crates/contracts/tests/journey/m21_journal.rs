use contracts::{CommentId, ENTRY_EVENTS, Entry, PostId, UserId, Visibility, decode, write_line};
use serde_json::{Value, json};

fn every_entry() -> Vec<Entry> {
    vec![
        Entry::Registered {
            user: UserId(1),
            name: "alice".to_string(),
        },
        Entry::Followed {
            who: UserId(2),
            whom: UserId(1),
        },
        Entry::Muted {
            who: UserId(1),
            whom: UserId(2),
        },
        Entry::Posted {
            post: PostId(1),
            author: UserId(1),
            text: "Привет\nмир".to_string(),
            visibility: Visibility::Followers,
            at: 1_700_000_000,
        },
        Entry::Liked {
            post: PostId(1),
            by: UserId(2),
            at: 1_700_000_005,
        },
        Entry::Deleted {
            post: PostId(1),
            by: UserId(1),
        },
        Entry::Commented {
            comment: CommentId(1),
            post: PostId(1),
            author: UserId(2),
            text: "класс".to_string(),
        },
        Entry::Replied {
            comment: CommentId(2),
            parent: CommentId(1),
            author: UserId(1),
            text: "спасибо".to_string(),
        },
        Entry::MessageSent {
            from: UserId(2),
            to: UserId(1),
            text: "привет".to_string(),
        },
    ]
}

fn value(entry: &Entry) -> Value {
    serde_json::to_value(entry).expect("запись журнала сериализуется")
}

#[test]
fn every_entry_names_its_event() {
    let entries = every_entry();
    assert_eq!(
        entries.iter().map(Entry::event).collect::<Vec<_>>(),
        ENTRY_EVENTS,
        "ENTRY_EVENTS перечисляет события журнала в порядке вариантов Entry"
    );
    for entry in &entries {
        assert_eq!(
            value(entry)["event"],
            entry.event(),
            "поле event в JSON совпадает с Entry::event()"
        );
    }
}

#[test]
fn posted_entry_has_the_agreed_shape() {
    let entries = every_entry();
    assert_eq!(
        value(&entries[3]),
        json!({
            "event": "posted",
            "post": "p_1",
            "author": "u_1",
            "text": "Привет\nмир",
            "visibility": "followers",
            "at": 1_700_000_000u64,
        }),
        "пост в журнале: id строками как на проводе, видимость словом, время в секундах"
    );
    assert_eq!(
        value(&entries[0]),
        json!({"event": "registered", "user": "u_1", "name": "alice"}),
    );
    assert_eq!(
        value(&entries[7]),
        json!({"event": "replied", "comment": "c_2", "parent": "c_1", "author": "u_1", "text": "спасибо"}),
    );
}

#[test]
fn entries_roundtrip_through_one_line_each() {
    for entry in every_entry() {
        let mut line = Vec::new();
        write_line(&mut line, &entry).expect("запись в Vec не падает");
        assert_eq!(
            line.iter().filter(|byte| **byte == b'\n').count(),
            1,
            "{}: одна запись это ровно одна строка, перевод строки в тексте экранирован",
            entry.event()
        );
        let text = std::str::from_utf8(&line).expect("журнал пишется в UTF-8");
        assert_eq!(
            decode::<Entry>(text.trim_end()).expect("строка журнала читается обратно"),
            entry,
            "запись возвращается той же"
        );
    }
}

#[test]
fn visibility_is_a_lowercase_word() {
    for (visibility, word) in [
        (Visibility::Public, "public"),
        (Visibility::Followers, "followers"),
        (Visibility::Private, "private"),
    ] {
        assert_eq!(
            serde_json::to_value(visibility).expect("видимость сериализуется"),
            json!(word)
        );
    }
}

#[test]
fn unknown_event_is_refused() {
    assert!(
        decode::<Entry>(r#"{"event": "exploded", "user": "u_1"}"#).is_err(),
        "неизвестное событие в журнале это ошибка, а не пропуск"
    );
    assert!(
        decode::<Entry>(r#"{"event": "followed", "who": "u_1"}"#).is_err(),
        "запись без обязательного поля это ошибка"
    );
}
