use std::io::{Read, Write};
use std::process::{Command as Process, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use backend::{
    Command, CommandError, DomainError, MAX_NAME_LEN, MemNetwork, MemPostStore, MemUserStore,
    ParseError, PostId, SharedNetwork, Username,
};

fn name(raw: &str) -> Username {
    Username::try_from(raw).expect("нормальный ник должен проходить проверку")
}

fn parse(line: &str) -> Result<Command, CommandError> {
    line.parse::<Command>()
}

fn shared() -> SharedNetwork<MemUserStore, MemPostStore> {
    SharedNetwork::new(MemNetwork::new()).expect("фоновый поток уведомлений должен запуститься")
}

fn run(net: &SharedNetwork<MemUserStore, MemPostStore>, line: &str) -> Result<String, CommandError> {
    parse(line).and_then(|command| command.run(net))
}

#[test]
fn username_is_checked_on_the_way_in() {
    let alice = name("  Alice ");
    let as_str: &str = alice.as_ref();
    assert_eq!(as_str, "alice", "Username хранит нормализованный ник и отдаёт его через AsRef<str>");
    assert_eq!(alice.to_string(), "@alice", "Display у Username печатает @ник");
    assert_eq!(Username::try_from("   "), Err(DomainError::EmptyName), "пустой ник не становится Username");
    assert_eq!(
        Username::try_from("a".repeat(MAX_NAME_LEN + 1).as_str()),
        Err(DomainError::NameTooLong),
        "слишком длинный ник не становится Username"
    );
}

#[test]
fn commands_parse_from_text() {
    assert_eq!(parse("register Alice"), Ok(Command::Register(name("alice"))), "register <ник>");
    assert_eq!(parse("  inbox   bob  "), Ok(Command::Inbox(name("bob"))), "лишние пробелы вокруг слов не мешают");
    assert_eq!(
        parse("post alice Привет,   мир! #rust"),
        Ok(Command::Post { author: name("alice"), text: String::from("Привет,   мир! #rust") }),
        "post <ник> <текст>: текст это вся остальная строка, пробелы внутри сохраняются"
    );
    assert_eq!(parse("follow alice BOB"), Ok(Command::Follow { who: name("alice"), whom: name("bob") }), "follow <ник> <на кого>");
    assert_eq!(parse("like bob 7"), Ok(Command::Like { who: name("bob"), post: PostId(7) }), "like <ник> <id поста>");
    assert_eq!(parse("feed alice"), Ok(Command::Feed { who: name("alice"), limit: 10 }), "feed <ник>: по умолчанию 10 постов");
    assert_eq!(parse("feed alice 3"), Ok(Command::Feed { who: name("alice"), limit: 3 }), "feed <ник> <сколько>");
}

#[test]
fn broken_commands_explain_themselves() {
    let cases = [
        ("", ParseError::Empty),
        ("dance alice", ParseError::UnknownCommand(String::from("dance"))),
        ("register", ParseError::MissingArgument("ник")),
        ("register alice bob", ParseError::ExtraArgument(String::from("bob"))),
        ("post alice", ParseError::MissingArgument("текст поста")),
        ("follow alice", ParseError::MissingArgument("на кого")),
        ("follow alice bob carol", ParseError::ExtraArgument(String::from("carol"))),
        ("like bob", ParseError::MissingArgument("id поста")),
        ("like bob seven", ParseError::BadNumber(String::from("seven"))),
        ("feed alice many", ParseError::BadNumber(String::from("many"))),
        ("inbox", ParseError::MissingArgument("ник")),
    ];
    for (line, error) in cases {
        assert_eq!(parse(line), Err(CommandError::Parse(error.clone())), "строка {line:?} должна дать {error:?}");
    }
}

#[test]
fn errors_of_both_layers_meet_in_command_error() {
    assert_eq!(
        parse(&format!("register {}", "a".repeat(MAX_NAME_LEN + 1))),
        Err(CommandError::Domain(DomainError::NameTooLong)),
        "ошибка проверки ника при разборе приходит как CommandError::Domain"
    );
    assert_eq!(CommandError::from(ParseError::Empty), CommandError::Parse(ParseError::Empty), "From<ParseError> для CommandError");
    assert_eq!(
        CommandError::from(DomainError::SelfFollow),
        CommandError::Domain(DomainError::SelfFollow),
        "From<DomainError> для CommandError"
    );
    let texts = [
        (CommandError::Parse(ParseError::Empty), "пустая команда"),
        (CommandError::Parse(ParseError::UnknownCommand(String::from("dance"))), "неизвестная команда: dance"),
        (CommandError::Parse(ParseError::MissingArgument("ник")), "не хватает аргумента: ник"),
        (CommandError::Parse(ParseError::ExtraArgument(String::from("bob"))), "лишний аргумент: bob"),
        (CommandError::Parse(ParseError::BadNumber(String::from("seven"))), "не число: seven"),
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
    assert_eq!(run(&net, "follow alice ghost"), Err(CommandError::UnknownName(String::from("ghost"))), "незнакомый ник: UnknownName");
    assert_eq!(run(&net, "follow alice alice"), Err(CommandError::Domain(DomainError::SelfFollow)), "подписка на себя");
    assert_eq!(
        run(&net, "like alice 42"),
        Err(CommandError::Domain(DomainError::UnknownPost(PostId(42)))),
        "лайк несуществующего поста"
    );
    assert_eq!(run(&net, "post alice    "), Err(CommandError::Parse(ParseError::MissingArgument("текст поста"))), "пост без текста");
}

#[test]
fn terminal_talks_through_stdin_and_stdout() {
    let script = "register alice\nregister bob\n\npost alice Первый пост #hello\nfollow bob alice\nlike bob 1\nfeed bob\ninbox alice\ndance\nfollow bob ghost\n";
    let expected = "зарегистрирован @alice (id 1)\n\
                    зарегистрирован @bob (id 2)\n\
                    опубликован пост [1]\n\
                    @bob подписан на @alice\n\
                    @bob лайкнул пост [1]\n\
                    @alice [1] Первый пост #hello (лайков: 1)\n\
                    @bob подписался на тебя\n\
                    @bob лайкнул твой пост [1]\n\
                    ошибка: неизвестная команда: dance\n\
                    ошибка: нет пользователя @ghost\n";
    let mut child = Process::new(env!("CARGO_BIN_EXE_backend"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("бинарник backend (src/main.rs) должен запускаться");
    child
        .stdin
        .take()
        .expect("у процесса есть stdin")
        .write_all(script.as_bytes())
        .expect("скрипт пишется в stdin");
    let deadline = Instant::now() + Duration::from_secs(10);
    let status = loop {
        match child.try_wait().expect("состояние процесса читается") {
            Some(status) => break status,
            None if Instant::now() > deadline => {
                let _ = child.kill();
                panic!("терминал не завершился за 10 секунд после конца stdin: цикл по строкам должен кончаться на EOF");
            }
            None => thread::sleep(Duration::from_millis(20)),
        }
    };
    let mut output = String::new();
    child
        .stdout
        .take()
        .expect("у процесса есть stdout")
        .read_to_string(&mut output)
        .expect("stdout читается");
    assert!(status.success(), "терминал должен завершаться успешно, когда stdin кончился");
    assert_eq!(output, expected, "терминал отвечает строкой на каждую непустую команду, ошибки печатает как `ошибка: ...`");
}
