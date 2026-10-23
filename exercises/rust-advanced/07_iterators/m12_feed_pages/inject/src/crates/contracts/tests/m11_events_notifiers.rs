use contracts::{CHAT_COMMAND_NAMES, COMMAND_NAMES, ChatCommand, Command, ParseError};

#[test]
fn network_command_names_match_the_parser() {
    assert_eq!(
        COMMAND_NAMES,
        [
            "register", "post", "follow", "like", "feed", "inbox", "mute", "delete", "page"
        ],
        "COMMAND_NAMES перечисляет сетевые команды в порядке из ТЗ"
    );
    for name in COMMAND_NAMES {
        assert!(
            !matches!(name.parse::<Command>(), Err(ParseError::UnknownCommand(_))),
            "разбор Command должен знать команду {name:?} из COMMAND_NAMES"
        );
    }
}

#[test]
fn chat_command_names_match_the_parser() {
    assert_eq!(
        CHAT_COMMAND_NAMES,
        ["/login", "/logout", "/whoami", "/msg", "/say", "/history"],
        "CHAT_COMMAND_NAMES перечисляет команды сессии вместе со слэшем"
    );
    for name in CHAT_COMMAND_NAMES {
        assert!(
            !matches!(
                name.parse::<ChatCommand>(),
                Err(ParseError::UnknownCommand(_))
            ),
            "разбор ChatCommand должен знать команду {name:?} из CHAT_COMMAND_NAMES"
        );
    }
}
