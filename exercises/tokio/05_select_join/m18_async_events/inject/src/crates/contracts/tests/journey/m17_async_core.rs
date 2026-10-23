use contracts::{COMMAND_NAMES, Command, ParseError};

#[test]
fn import_is_listed_after_stats() {
    assert_eq!(
        COMMAND_NAMES.get(14..16),
        Some(&["stats", "import"][..]),
        "import дописан в COMMAND_NAMES сразу после stats, новые вехи дописывают свои в конец"
    );
}

#[test]
fn import_takes_the_rest_of_the_line_as_a_path() {
    let cases = [
        ("import seed.txt", "seed.txt"),
        ("import   data/seed.txt  ", "data/seed.txt"),
        ("import мои данные.txt", "мои данные.txt"),
    ];
    for (line, path) in cases {
        assert_eq!(
            line.parse::<Command>(),
            Ok(Command::Import {
                path: String::from(path)
            }),
            "{line:?}: путь это весь остаток строки без пробелов по краям"
        );
    }
}

#[test]
fn import_needs_a_path() {
    for line in ["import", "import    "] {
        assert_eq!(
            line.parse::<Command>(),
            Err(ParseError::MissingArgument("путь к файлу")),
            "{line:?}: без пути импортировать нечего"
        );
    }
}
