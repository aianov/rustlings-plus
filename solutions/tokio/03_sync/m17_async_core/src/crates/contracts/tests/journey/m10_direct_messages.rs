use std::panic;

use contracts::{
    ChatCommand, Command, DEFAULT_HISTORY_LIMIT, Input, MAX_HISTORY_LIMIT, MAX_NAME_LEN, NameError,
    ParseError, Username,
};

fn name(raw: &str) -> Username {
    Username::try_from(raw).expect("нормальный ник должен проходить проверку")
}

fn chat(line: &str) -> Result<ChatCommand, ParseError> {
    line.parse::<ChatCommand>()
}

fn msg(to: &str, text: &str) -> ChatCommand {
    ChatCommand::Msg {
        to: name(to),
        text: String::from(text),
    }
}

fn history(with: &str, limit: usize) -> ChatCommand {
    ChatCommand::History {
        with: name(with),
        limit,
    }
}

#[test]
fn history_limits_are_fixed() {
    assert_eq!(
        DEFAULT_HISTORY_LIMIT, 20,
        "/history без числа показывает 20 сообщений"
    );
    assert_eq!(
        MAX_HISTORY_LIMIT, 100,
        "больше 100 сообщений за раз /history не показывает"
    );
}

#[test]
fn session_commands_parse_from_slash_lines() {
    let cases = [
        ("/login Alice", ChatCommand::Login(name("alice"))),
        ("  /logout  ", ChatCommand::Logout),
        ("/whoami", ChatCommand::Whoami),
        ("/msg bob привет", msg("bob", "привет")),
        ("/msg   Bob   как   дела?  ", msg("bob", "как дела?")),
        ("/msg bob 1/2 /quit", msg("bob", "1/2 /quit")),
        (
            "/say ну что,   идём?",
            ChatCommand::Say(String::from("ну что, идём?")),
        ),
        ("/history bob", history("bob", DEFAULT_HISTORY_LIMIT)),
        ("/history bob 5", history("bob", 5)),
        ("/history bob 1", history("bob", 1)),
        ("/history bob 100", history("bob", MAX_HISTORY_LIMIT)),
    ];
    for (line, expected) in cases {
        assert_eq!(
            chat(line),
            Ok(expected.clone()),
            "строка {line:?} должна дать {expected:?} (слова текста склеиваются одним пробелом)"
        );
    }
}

#[test]
fn broken_session_commands_explain_themselves() {
    let long = "a".repeat(MAX_NAME_LEN + 1);
    let cases = [
        (String::from(""), ParseError::Empty),
        (String::from("   "), ParseError::Empty),
        (
            String::from("login alice"),
            ParseError::UnknownCommand(String::from("login")),
        ),
        (
            String::from("/"),
            ParseError::UnknownCommand(String::from("/")),
        ),
        (
            String::from("/dance"),
            ParseError::UnknownCommand(String::from("/dance")),
        ),
        (
            String::from("/Login alice"),
            ParseError::UnknownCommand(String::from("/Login")),
        ),
        (String::from("/login"), ParseError::MissingArgument("ник")),
        (
            String::from("/login alice bob"),
            ParseError::ExtraArgument(String::from("bob")),
        ),
        (
            String::from("/logout now"),
            ParseError::ExtraArgument(String::from("now")),
        ),
        (
            String::from("/whoami please"),
            ParseError::ExtraArgument(String::from("please")),
        ),
        (String::from("/msg"), ParseError::MissingArgument("кому")),
        (
            String::from("/msg bob"),
            ParseError::MissingArgument("текст сообщения"),
        ),
        (
            String::from("/msg bob    "),
            ParseError::MissingArgument("текст сообщения"),
        ),
        (
            String::from("/say"),
            ParseError::MissingArgument("текст сообщения"),
        ),
        (
            String::from("/history"),
            ParseError::MissingArgument("с кем"),
        ),
        (
            String::from("/history bob many"),
            ParseError::BadNumber(String::from("many")),
        ),
        (
            String::from("/history bob -3"),
            ParseError::BadNumber(String::from("-3")),
        ),
        (
            String::from("/history bob 99999999999999999999999"),
            ParseError::BadNumber(String::from("99999999999999999999999")),
        ),
        (
            String::from("/history bob 0"),
            ParseError::LimitOutOfRange(0),
        ),
        (
            String::from("/history bob 101"),
            ParseError::LimitOutOfRange(101),
        ),
        (
            String::from("/history bob 5 6"),
            ParseError::ExtraArgument(String::from("6")),
        ),
        (
            format!("/login {long}"),
            ParseError::BadName(NameError::TooLong),
        ),
        (
            format!("/msg {long} привет"),
            ParseError::BadName(NameError::TooLong),
        ),
    ];
    for (line, error) in cases {
        assert_eq!(
            chat(&line),
            Err(error.clone()),
            "строка {line:?} должна дать {error:?}"
        );
    }
}

#[test]
fn shape_is_checked_before_content() {
    let long = "a".repeat(MAX_NAME_LEN + 1);
    let cases = [
        (
            format!("/login {long} extra"),
            ParseError::ExtraArgument(String::from("extra")),
            "лишний аргумент заметен раньше плохого ника",
        ),
        (
            format!("/msg {long}"),
            ParseError::MissingArgument("текст сообщения"),
            "нехватка текста заметна раньше плохого ника",
        ),
        (
            String::from("/history bob 0 x"),
            ParseError::ExtraArgument(String::from("x")),
            "лишний аргумент заметен раньше плохого числа",
        ),
        (
            format!("/history {long} many"),
            ParseError::BadName(NameError::TooLong),
            "ник проверяется раньше числа",
        ),
        (
            String::from("/dance bob"),
            ParseError::UnknownCommand(String::from("/dance")),
            "неизвестная команда заметна раньше всего остального",
        ),
    ];
    for (line, error, why) in cases {
        assert_eq!(chat(&line), Err(error), "строка {line:?}: {why}");
    }
}

#[test]
fn input_tells_session_commands_from_network_commands() {
    let cases = [
        (
            "register Alice",
            Ok(Input::Command(Command::Register(name("alice")))),
        ),
        ("   /whoami", Ok(Input::Chat(ChatCommand::Whoami))),
        ("/msg bob привет", Ok(Input::Chat(msg("bob", "привет")))),
        ("", Err(ParseError::Empty)),
        (
            "dance",
            Err(ParseError::UnknownCommand(String::from("dance"))),
        ),
        (
            "/dance",
            Err(ParseError::UnknownCommand(String::from("/dance"))),
        ),
    ];
    for (line, expected) in cases {
        assert_eq!(
            line.parse::<Input>(),
            expected.clone(),
            "строка {line:?}: команды с / идут в сессию, остальные в сеть, итог {expected:?}"
        );
    }
    assert_eq!(
        "/msg bob привет".parse::<Command>(),
        Err(ParseError::UnknownCommand(String::from("/msg"))),
        "разбор сетевых команд остался прежним: /msg он не знает"
    );
}

#[test]
fn network_commands_carry_messages() {
    let send = Command::SendMessage {
        from: name("alice"),
        to: name("bob"),
        text: String::from("привет"),
    };
    let show = Command::History {
        who: name("alice"),
        with: name("bob"),
        limit: 5,
    };
    assert_ne!(
        send, show,
        "в Command есть SendMessage {{ from, to, text }} и History {{ who, with, limit }}"
    );
}

#[test]
fn parser_never_panics() {
    let lines = [
        "/",
        "//",
        "/ /",
        "/msg /msg /msg",
        "/login 👍",
        "/msg 💬 текст",
        "\t/whoami\t",
        "/history bob 1 2 3 4",
        "/say /",
        "/история bob",
        "/msg\u{a0}bob привет",
        "post",
        "feed alice -1",
    ];
    for line in lines {
        let parsed = panic::catch_unwind(|| line.parse::<Input>());
        assert!(
            parsed.is_ok(),
            "разбор строки {line:?} паникует, а должен вернуть Ok или Err"
        );
    }
}

#[test]
fn limit_error_reads_like_a_sentence() {
    assert_eq!(
        ParseError::LimitOutOfRange(0).to_string(),
        "лимит истории от 1 до 100, а не 0",
        "Display для ParseError::LimitOutOfRange"
    );
}
