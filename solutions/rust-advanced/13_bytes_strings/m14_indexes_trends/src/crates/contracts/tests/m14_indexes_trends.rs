use std::borrow::Cow;

use contracts::{
    COMMAND_NAMES, Command, CommentId, Cursor, DEFAULT_FEED_LIMIT, Envelope, Error, MAX_NAME_LEN,
    NameError, ParseError, PostId, UserId, Username, decode, decode_call, normalize_name,
};

fn name(raw: &str) -> Username {
    Username::try_from(raw).expect("нормальный ник проходит проверку")
}

fn parse(line: &str) -> Result<Command, ParseError> {
    line.parse::<Command>()
}

#[test]
fn nick_is_latin_digits_and_underscore() {
    for raw in ["alice", "bob_42", "_", "x1", "007"] {
        assert_eq!(name(raw).as_ref(), raw, "ник {raw:?} проходит как есть");
    }
    assert_eq!(
        name("  Bob_42 ").as_ref(),
        "bob_42",
        "ник обрезается по краям и переводится в нижний регистр"
    );
    let longest = "A".repeat(MAX_NAME_LEN);
    assert_eq!(
        name(&longest).as_ref(),
        "a".repeat(MAX_NAME_LEN),
        "ник ровно из MAX_NAME_LEN символов разрешён"
    );
}

#[test]
fn first_foreign_char_is_named() {
    let cases = [
        ("алиса", 'а'),
        ("bob-smith", '-'),
        ("bob smith", ' '),
        ("naïve", 'ï'),
        ("user１", '１'),
        ("ÄLICE", 'Ä'),
        ("İvan", 'İ'),
        ("bob.", '.'),
        ("👍", '👍'),
    ];
    for (raw, bad) in cases {
        assert_eq!(
            Username::try_from(raw),
            Err(NameError::BadChar(bad)),
            "в {raw:?} первый неподходящий символ {bad:?}"
        );
    }
    assert_eq!(
        NameError::BadChar('я').to_string(),
        "в нике можно только a-z, 0-9 и _, а 'я' нельзя",
        "Display для NameError::BadChar"
    );
}

#[test]
fn checks_go_empty_then_chars_then_length() {
    assert_eq!(
        Username::try_from("   "),
        Err(NameError::Empty),
        "пустой ник"
    );
    assert_eq!(
        Username::try_from("я".repeat(MAX_NAME_LEN + 1).as_str()),
        Err(NameError::BadChar('я')),
        "символы проверяются раньше длины"
    );
    assert_eq!(
        Username::try_from("a".repeat(MAX_NAME_LEN + 1).as_str()),
        Err(NameError::TooLong),
        "длинный ник"
    );
}

#[test]
fn normalize_name_is_ascii_only() {
    assert!(
        matches!(normalize_name(" alice_1 "), Cow::Borrowed("alice_1")),
        "без заглавных букв копии нет"
    );
    assert!(
        matches!(normalize_name("ALICE"), Cow::Owned(ref name) if name == "alice"),
        "заглавные ASCII переводятся в нижний регистр"
    );
    let mixed = "ÄLİCE";
    let normal = normalize_name(mixed);
    assert_eq!(
        normal, "Älİce",
        "to_ascii_lowercase трогает только ASCII: Ä и İ остаются как были"
    );
    assert_eq!(
        normal.len(),
        mixed.len(),
        "ASCII-нижний регистр не меняет длину (to_lowercase превратил бы İ в два символа)"
    );
}

#[test]
fn nick_rules_hold_on_the_wire() {
    let line = r#"{"id": 1, "service": "users", "method": "users.register", "data": {"username": "алиса"}}"#;
    let envelope: Envelope = decode(line).expect("конверт разбирается");
    assert_eq!(
        decode_call(&envelope),
        Err(Error::BadData {
            method: String::from("users.register"),
            reason: String::from("в нике можно только a-z, 0-9 и _, а 'а' нельзя"),
        }),
        "ник с провода проходит те же проверки"
    );
}

#[test]
fn ids_are_ordered_by_number() {
    assert!(PostId(2) < PostId(10), "PostId сравнивается по числу");
    assert!(UserId(1) < UserId(2), "UserId сравнивается по числу");
    assert!(
        CommentId(3) < CommentId(30),
        "CommentId сравнивается по числу"
    );
    let mut ids = [PostId(10), PostId(2), PostId(7)];
    ids.sort();
    assert_eq!(ids, [PostId(2), PostId(7), PostId(10)], "посты сортируются");
}

#[test]
fn comment_id_on_the_wire() {
    assert_eq!(CommentId(5).to_string(), "c_5", "Display для CommentId");
    assert_eq!(
        "c_5".parse::<CommentId>(),
        Ok(CommentId(5)),
        "FromStr для CommentId"
    );
    for bad in ["5", "c_", "c_+5", "p_5", "C_5", "c_5 "] {
        let error = bad
            .parse::<CommentId>()
            .expect_err("кривой id комментария не разбирается");
        assert_eq!(
            error.to_string(),
            format!("неверный id {bad:?}: нужно c_<число>"),
            "текст ошибки для {bad:?}"
        );
    }
    assert_eq!(
        serde_json::to_string(&CommentId(3)).expect("CommentId пишется"),
        r#""c_3""#,
        "на проводе CommentId это строка"
    );
    assert_eq!(
        serde_json::from_str::<CommentId>(r#""c_3""#).expect("CommentId читается"),
        CommentId(3),
        "и читается обратно"
    );
}

#[test]
fn cursor_orders_by_time_then_post() {
    let a = Cursor {
        time: 100,
        post: PostId(9),
    };
    let b = Cursor {
        time: 100,
        post: PostId(10),
    };
    let c = Cursor {
        time: 101,
        post: PostId(1),
    };
    assert!(a < b, "при одинаковом времени новее больший id");
    assert!(b < c, "время сравнивается первым");
}

#[test]
fn cursor_as_text() {
    let cursor = Cursor {
        time: 1_700_000_000,
        post: PostId(42),
    };
    assert_eq!(cursor.to_string(), "1700000000:42", "Display курсора");
    assert_eq!(
        "1700000000:42".parse::<Cursor>(),
        Ok(cursor),
        "FromStr курсора"
    );
    for bad in [
        "",
        "100",
        "100:",
        ":5",
        "100:+5",
        "+100:5",
        "100:5:6",
        "100 :5",
        "a:b",
        "99999999999999999999:1",
    ] {
        assert_eq!(
            bad.parse::<Cursor>(),
            Err(ParseError::BadCursor(bad.to_string())),
            "курсор {bad:?} не разбирается: только <цифры>:<цифры>"
        );
    }
    assert_eq!(
        ParseError::BadCursor(String::from("100")).to_string(),
        r#"неверный курсор "100": нужно <время>:<id поста>"#,
        "Display для ParseError::BadCursor"
    );
}

#[test]
fn new_commands_are_listed() {
    assert_eq!(
        COMMAND_NAMES,
        [
            "register", "post", "follow", "like", "feed", "inbox", "mute", "delete", "page",
            "posts", "comment", "reply", "thread", "trends"
        ],
        "COMMAND_NAMES дополнен пятью командами в порядке из ТЗ"
    );
}

#[test]
fn new_commands_parse() {
    let cursor = Cursor {
        time: 100,
        post: PostId(7),
    };
    let cases = [
        (
            "posts bob alice",
            Command::Posts {
                who: name("bob"),
                author: name("alice"),
                size: DEFAULT_FEED_LIMIT,
                after: None,
            },
        ),
        (
            "posts bob Alice 3",
            Command::Posts {
                who: name("bob"),
                author: name("alice"),
                size: 3,
                after: None,
            },
        ),
        (
            "posts bob alice 3 100:7",
            Command::Posts {
                who: name("bob"),
                author: name("alice"),
                size: 3,
                after: Some(cursor),
            },
        ),
        (
            "comment bob 7 Отличный   пост!",
            Command::Comment {
                who: name("bob"),
                post: PostId(7),
                text: String::from("Отличный   пост!"),
            },
        ),
        (
            "reply alice 2 Спасибо",
            Command::Reply {
                who: name("alice"),
                to: CommentId(2),
                text: String::from("Спасибо"),
            },
        ),
        (
            "thread bob 7",
            Command::Thread {
                who: name("bob"),
                post: PostId(7),
            },
        ),
        (
            "trends",
            Command::Trends {
                limit: DEFAULT_FEED_LIMIT,
            },
        ),
        ("trends 3", Command::Trends { limit: 3 }),
    ];
    for (line, expected) in cases {
        assert_eq!(parse(line), Ok(expected), "разбор {line:?}");
    }
}

#[test]
fn new_commands_reject_bad_input() {
    let cases = [
        ("posts", ParseError::MissingArgument("ник")),
        ("posts bob", ParseError::MissingArgument("чьи посты")),
        (
            "posts bob alice x",
            ParseError::BadNumber(String::from("x")),
        ),
        (
            "posts bob alice 3 7",
            ParseError::BadCursor(String::from("7")),
        ),
        (
            "posts bob alice 3 100:7 more",
            ParseError::ExtraArgument(String::from("more")),
        ),
        ("comment bob", ParseError::MissingArgument("id поста")),
        (
            "comment bob 7",
            ParseError::MissingArgument("текст комментария"),
        ),
        (
            "comment bob p7 текст",
            ParseError::BadNumber(String::from("p7")),
        ),
        ("reply alice", ParseError::MissingArgument("id комментария")),
        ("reply alice 2", ParseError::MissingArgument("текст ответа")),
        ("thread bob", ParseError::MissingArgument("id поста")),
        (
            "thread bob 7 8",
            ParseError::ExtraArgument(String::from("8")),
        ),
        ("trends x", ParseError::BadNumber(String::from("x"))),
        ("trends 3 4", ParseError::ExtraArgument(String::from("4"))),
        (
            "comment алиса 7 текст",
            ParseError::BadName(NameError::BadChar('а')),
        ),
    ];
    for (line, expected) in cases {
        assert_eq!(parse(line), Err(expected), "разбор {line:?}");
    }
}
