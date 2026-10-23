use contracts::{COMMAND_NAMES, Command, ParseError, SETTING_NAMES, Setting, Username};

fn name(raw: &str) -> Username {
    Username::try_from(raw).expect("нормальный ник проходит проверку")
}

#[test]
fn set_and_card_are_listed_after_import() {
    assert_eq!(
        COMMAND_NAMES.get(15..18),
        Some(&["import", "set", "card"][..]),
        "set и card дописаны в COMMAND_NAMES после import"
    );
}

#[test]
fn settings_have_names() {
    assert_eq!(SETTING_NAMES, ["post_limit", "trends_window"]);
    for (text, setting) in [
        ("post_limit", Setting::PostLimit),
        ("trends_window", Setting::TrendsWindow),
    ] {
        assert_eq!(
            text.parse::<Setting>(),
            Ok(setting),
            "{text:?} это {setting:?}"
        );
        assert_eq!(setting.to_string(), text, "Display настройки это её имя");
        assert_eq!(setting.name(), text);
    }
    assert_eq!(
        "theme".parse::<Setting>(),
        Err(ParseError::UnknownSetting(String::from("theme")))
    );
    assert_eq!(
        ParseError::UnknownSetting(String::from("theme")).to_string(),
        "неизвестная настройка \"theme\", есть post_limit, trends_window",
        "ошибка перечисляет настройки, которые есть"
    );
}

#[test]
fn set_takes_a_setting_and_a_number() {
    assert_eq!(
        "set post_limit 140".parse::<Command>(),
        Ok(Command::Set {
            setting: Setting::PostLimit,
            value: 140
        })
    );
    assert_eq!(
        "set   trends_window   3600".parse::<Command>(),
        Ok(Command::Set {
            setting: Setting::TrendsWindow,
            value: 3600
        })
    );
    let errors = [
        ("set", ParseError::MissingArgument("настройка")),
        ("set post_limit", ParseError::MissingArgument("значение")),
        (
            "set post_limit много",
            ParseError::BadNumber(String::from("много")),
        ),
        (
            "set color 1",
            ParseError::UnknownSetting(String::from("color")),
        ),
        (
            "set post_limit 1 2",
            ParseError::ExtraArgument(String::from("2")),
        ),
    ];
    for (line, error) in errors {
        assert_eq!(line.parse::<Command>(), Err(error), "{line:?}");
    }
}

#[test]
fn card_takes_two_names() {
    assert_eq!(
        "card bob Alice".parse::<Command>(),
        Ok(Command::Card {
            who: name("bob"),
            whose: name("alice")
        }),
        "card <кто смотрит> <чья карточка>, ники нормализуются"
    );
    assert_eq!(
        "card bob".parse::<Command>(),
        Err(ParseError::MissingArgument("чья карточка"))
    );
    assert_eq!(
        "card bob alice carol".parse::<Command>(),
        Err(ParseError::ExtraArgument(String::from("carol")))
    );
}
