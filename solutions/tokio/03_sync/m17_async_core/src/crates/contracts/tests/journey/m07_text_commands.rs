use contracts::{Command, MAX_NAME_LEN, NameError, ParseError, PostId, Username};

fn name(raw: &str) -> Username {
    Username::try_from(raw).expect("нормальный ник должен проходить проверку")
}

fn parse(line: &str) -> Result<Command, ParseError> {
    line.parse::<Command>()
}

#[test]
fn username_is_checked_on_the_way_in() {
    let alice = name("  Alice ");
    let as_str: &str = alice.as_ref();
    assert_eq!(
        as_str, "alice",
        "Username хранит нормализованный ник и отдаёт его через AsRef<str>"
    );
    assert_eq!(
        alice.to_string(),
        "@alice",
        "Display у Username печатает @ник"
    );
    assert_eq!(
        Username::try_from("   "),
        Err(NameError::Empty),
        "пустой ник не становится Username"
    );
    assert_eq!(
        Username::try_from("a".repeat(MAX_NAME_LEN + 1).as_str()),
        Err(NameError::TooLong),
        "слишком длинный ник не становится Username"
    );
}

#[test]
fn commands_parse_from_text() {
    assert_eq!(
        parse("register Alice"),
        Ok(Command::Register(name("alice"))),
        "register <ник>"
    );
    assert_eq!(
        parse("  inbox   bob  "),
        Ok(Command::Inbox(name("bob"))),
        "лишние пробелы вокруг слов не мешают"
    );
    assert_eq!(
        parse("post alice Привет,   мир! #rust"),
        Ok(Command::Post {
            author: name("alice"),
            text: String::from("Привет,   мир! #rust")
        }),
        "post <ник> <текст>: текст это вся остальная строка, пробелы внутри сохраняются"
    );
    assert_eq!(
        parse("follow alice BOB"),
        Ok(Command::Follow {
            who: name("alice"),
            whom: name("bob")
        }),
        "follow <ник> <на кого>"
    );
    assert_eq!(
        parse("like bob 7"),
        Ok(Command::Like {
            who: name("bob"),
            post: PostId(7)
        }),
        "like <ник> <id поста>"
    );
    assert_eq!(
        parse("feed alice"),
        Ok(Command::Feed {
            who: name("alice"),
            limit: 10
        }),
        "feed <ник>: по умолчанию 10 постов"
    );
    assert_eq!(
        parse("feed alice 3"),
        Ok(Command::Feed {
            who: name("alice"),
            limit: 3
        }),
        "feed <ник> <сколько>"
    );
}

#[test]
fn broken_commands_explain_themselves() {
    let cases = [
        ("", ParseError::Empty),
        (
            "dance alice",
            ParseError::UnknownCommand(String::from("dance")),
        ),
        ("register", ParseError::MissingArgument("ник")),
        (
            "register alice bob",
            ParseError::ExtraArgument(String::from("bob")),
        ),
        ("post alice", ParseError::MissingArgument("текст поста")),
        ("follow alice", ParseError::MissingArgument("на кого")),
        (
            "follow alice bob carol",
            ParseError::ExtraArgument(String::from("carol")),
        ),
        ("like bob", ParseError::MissingArgument("id поста")),
        (
            "like bob seven",
            ParseError::BadNumber(String::from("seven")),
        ),
        (
            "feed alice many",
            ParseError::BadNumber(String::from("many")),
        ),
        ("inbox", ParseError::MissingArgument("ник")),
    ];
    for (line, error) in cases {
        assert_eq!(
            parse(line),
            Err(error.clone()),
            "строка {line:?} должна дать {error:?}"
        );
    }
}

#[test]
fn bad_name_is_a_parse_error() {
    assert_eq!(
        parse(&format!("register {}", "a".repeat(MAX_NAME_LEN + 1))),
        Err(ParseError::BadName(NameError::TooLong)),
        "плохой ник при разборе даёт ParseError::BadName с ошибкой ника внутри"
    );
}

#[test]
fn parse_errors_read_like_sentences() {
    let texts = [
        (ParseError::Empty, "пустая команда"),
        (
            ParseError::UnknownCommand(String::from("dance")),
            "неизвестная команда: dance",
        ),
        (
            ParseError::MissingArgument("ник"),
            "не хватает аргумента: ник",
        ),
        (
            ParseError::ExtraArgument(String::from("bob")),
            "лишний аргумент: bob",
        ),
        (
            ParseError::BadNumber(String::from("seven")),
            "не число: seven",
        ),
        (
            ParseError::BadName(NameError::Empty),
            "ник не может быть пустым",
        ),
        (
            ParseError::BadName(NameError::TooLong),
            "ник длиннее 20 символов",
        ),
    ];
    for (error, text) in texts {
        assert_eq!(error.to_string(), text, "Display для {error:?}");
    }
    let boxed: Box<dyn std::error::Error> = Box::new(ParseError::Empty);
    assert_eq!(
        boxed.to_string(),
        "пустая команда",
        "ParseError должен реализовать std::error::Error"
    );
}
