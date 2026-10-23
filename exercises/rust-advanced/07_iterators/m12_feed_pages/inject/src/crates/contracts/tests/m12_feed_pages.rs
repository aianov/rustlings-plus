use contracts::{COMMAND_NAMES, Command, DEFAULT_FEED_LIMIT, ParseError, PostId, Username};

fn name(raw: &str) -> Username {
    Username::try_from(raw).expect("нормальный ник должен проходить проверку")
}

fn parse(line: &str) -> Result<Command, ParseError> {
    line.parse::<Command>()
}

#[test]
fn new_commands_are_known_by_name() {
    for command in ["mute", "delete", "page"] {
        assert!(
            COMMAND_NAMES.contains(&command),
            "команда {command:?} есть в COMMAND_NAMES, и шина её зарегистрирует"
        );
    }
}

#[test]
fn mute_delete_and_page_parse() {
    let cases = [
        (
            "mute alice Bob",
            Command::Mute {
                who: name("alice"),
                whom: name("bob"),
            },
        ),
        (
            "delete alice 3",
            Command::Delete {
                who: name("alice"),
                post: PostId(3),
            },
        ),
        (
            "page alice 2",
            Command::Page {
                who: name("alice"),
                number: 2,
                size: DEFAULT_FEED_LIMIT,
            },
        ),
        (
            "  page   alice 2   5 ",
            Command::Page {
                who: name("alice"),
                number: 2,
                size: 5,
            },
        ),
        (
            "page alice 0",
            Command::Page {
                who: name("alice"),
                number: 0,
                size: DEFAULT_FEED_LIMIT,
            },
        ),
    ];
    for (line, expected) in cases {
        assert_eq!(
            parse(line),
            Ok(expected.clone()),
            "строка {line:?} должна дать {expected:?}"
        );
    }
}

#[test]
fn broken_new_commands_explain_themselves() {
    let cases = [
        ("mute", ParseError::MissingArgument("ник")),
        ("mute alice", ParseError::MissingArgument("кого скрыть")),
        (
            "mute alice bob carol",
            ParseError::ExtraArgument(String::from("carol")),
        ),
        ("delete alice", ParseError::MissingArgument("id поста")),
        ("delete alice x", ParseError::BadNumber(String::from("x"))),
        (
            "delete alice 1 2",
            ParseError::ExtraArgument(String::from("2")),
        ),
        ("page alice", ParseError::MissingArgument("номер страницы")),
        ("page alice two", ParseError::BadNumber(String::from("two"))),
        ("page alice 1 x", ParseError::BadNumber(String::from("x"))),
        (
            "page alice 1 2 3",
            ParseError::ExtraArgument(String::from("3")),
        ),
    ];
    for (line, error) in cases {
        assert_eq!(
            parse(line),
            Err(error.clone()),
            "строка {line:?} должна дать {error:?}"
        );
    }
}
