use contracts::{
    CHAT_COMMAND_NAMES, ChatCommand, DEFAULT_FEED_LIMIT, MAX_HISTORY_LIMIT, ParseError,
};

#[test]
fn watch_commands_are_chat_commands() {
    assert_eq!(
        CHAT_COMMAND_NAMES[6..],
        ["/watch", "/unwatch"],
        "команды живой ленты дописаны в конец CHAT_COMMAND_NAMES"
    );
    let cases = [
        (
            "/watch",
            ChatCommand::Watch {
                recent: DEFAULT_FEED_LIMIT,
            },
        ),
        ("/watch 3", ChatCommand::Watch { recent: 3 }),
        (
            "  /watch   100  ",
            ChatCommand::Watch {
                recent: MAX_HISTORY_LIMIT,
            },
        ),
        ("/unwatch", ChatCommand::Unwatch),
    ];
    for (line, expected) in cases {
        assert_eq!(
            line.parse::<ChatCommand>(),
            Ok(expected),
            "разбор {line:?}: /watch без числа показывает DEFAULT_FEED_LIMIT последних постов"
        );
    }
}

#[test]
fn bad_watch_commands_are_errors() {
    let cases = [
        ("/watch 0", ParseError::LimitOutOfRange(0)),
        ("/watch 101", ParseError::LimitOutOfRange(101)),
        ("/watch много", ParseError::BadNumber(String::from("много"))),
        ("/watch 3 4", ParseError::ExtraArgument(String::from("4"))),
        (
            "/unwatch bob",
            ParseError::ExtraArgument(String::from("bob")),
        ),
    ];
    for (line, expected) in cases {
        assert_eq!(
            line.parse::<ChatCommand>(),
            Err(expected),
            "разбор {line:?}: число постов от 1 до 100, лишние слова это ошибка, как у /history"
        );
    }
}
