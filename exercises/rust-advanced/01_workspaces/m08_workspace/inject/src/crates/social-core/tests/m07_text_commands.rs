use contracts::{NameError, ParseError};
use social_core::{
    CommandError, DomainError, MAX_NAME_LEN, MemNetwork, MemPostStore, MemUserStore, PostId,
    SharedNetwork, execute_line,
};

fn shared() -> SharedNetwork<MemUserStore, MemPostStore> {
    SharedNetwork::new(MemNetwork::new()).expect("фоновый поток уведомлений должен запуститься")
}

fn run(net: &SharedNetwork<MemUserStore, MemPostStore>, line: &str) -> Result<String, CommandError> {
    execute_line(line, net)
}

#[test]
fn errors_of_both_layers_meet_in_command_error() {
    assert_eq!(CommandError::from(ParseError::Empty), CommandError::Parse(ParseError::Empty), "From<ParseError> для CommandError");
    assert_eq!(
        CommandError::from(DomainError::SelfFollow),
        CommandError::Domain(DomainError::SelfFollow),
        "From<DomainError> для CommandError"
    );
    let texts = [
        (CommandError::Parse(ParseError::UnknownCommand(String::from("dance"))), "неизвестная команда: dance"),
        (CommandError::Domain(DomainError::SelfFollow), "нельзя подписаться на самого себя"),
        (CommandError::UnknownName(String::from("ghost")), "нет пользователя @ghost"),
    ];
    for (error, text) in texts {
        assert_eq!(error.to_string(), text, "Display для {error:?}");
    }
    let boxed: Box<dyn std::error::Error> = Box::new(CommandError::Parse(ParseError::Empty));
    assert_eq!(boxed.to_string(), "пустая команда", "CommandError должен реализовать std::error::Error");
}

#[test]
fn commands_run_against_the_network() {
    let net = shared();
    let steps = [
        ("register Alice", "зарегистрирован @alice (id 1)"),
        ("register bob", "зарегистрирован @bob (id 2)"),
        ("post alice Привет, мир! #rust", "опубликован пост [1]"),
        ("follow bob alice", "@bob подписан на @alice"),
        ("follow bob alice", "@bob уже подписан на @alice"),
        ("like bob 1", "@bob лайкнул пост [1]"),
        ("like bob 1", "@bob уже лайкал пост [1]"),
        ("feed bob", "@alice [1] Привет, мир! #rust (лайков: 1)"),
        ("feed alice 0", "лента пуста"),
        ("inbox alice", "@bob подписался на тебя\n@bob лайкнул твой пост [1]"),
        ("inbox bob", "уведомлений нет"),
    ];
    for (line, reply) in steps {
        assert_eq!(run(&net, line), Ok(String::from(reply)), "ответ на команду {line:?}");
    }
}

#[test]
fn run_reports_errors() {
    let net = shared();
    run(&net, "register alice").expect("alice регистрируется");
    assert_eq!(
        run(&net, "register ALICE"),
        Err(CommandError::Domain(DomainError::NameTaken(String::from("alice")))),
        "ошибка сети приходит как CommandError::Domain"
    );
    assert_eq!(
        run(&net, &format!("register {}", "a".repeat(MAX_NAME_LEN + 1))),
        Err(CommandError::Parse(ParseError::BadName(NameError::TooLong))),
        "плохой ник ловится ещё при разборе"
    );
    assert_eq!(run(&net, "follow alice ghost"), Err(CommandError::UnknownName(String::from("ghost"))), "незнакомый ник: UnknownName");
    assert_eq!(run(&net, "follow alice alice"), Err(CommandError::Domain(DomainError::SelfFollow)), "подписка на себя");
    assert_eq!(
        run(&net, "like alice 42"),
        Err(CommandError::Domain(DomainError::UnknownPost(PostId(42)))),
        "лайк несуществующего поста"
    );
    assert_eq!(run(&net, "post alice    "), Err(CommandError::Parse(ParseError::MissingArgument("текст поста"))), "пост без текста");
}
