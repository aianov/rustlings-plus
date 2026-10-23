use contracts::{COMMAND_METHODS, COMMAND_NAMES, Command, ParseError};

#[test]
fn tasks_is_an_admin_command() {
    assert_eq!(
        COMMAND_NAMES[18], "tasks",
        "команда tasks дописана в конец COMMAND_NAMES"
    );
    assert_eq!(
        "tasks".parse::<Command>(),
        Ok(Command::Tasks),
        "tasks без аргументов"
    );
    assert_eq!(
        "tasks now".parse::<Command>(),
        Err(ParseError::ExtraArgument(String::from("now"))),
        "у tasks нет аргументов"
    );
    assert_eq!(
        Command::Tasks.method(),
        "admin.tasks",
        "дамп задач это метод админа"
    );
    assert!(
        COMMAND_METHODS.contains(&"admin.tasks"),
        "admin.tasks есть в COMMAND_METHODS"
    );
}
