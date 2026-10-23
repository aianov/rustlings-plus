use chat_session::command::{parse, Command, ParseError};
use chat_session::session::{Reason, Rejected, Reply, Session};

fn ok(line: &str) -> Command {
    parse(line).unwrap_or_else(|e| panic!("строка {line:?} должна разбираться, а получили {e:?}"))
}

fn err(line: &str) -> ParseError {
    parse(line).expect_err("такая строка не должна разбираться")
}

fn s(x: &str) -> String {
    x.to_string()
}

#[test]
fn plain_text_is_said() {
    assert_eq!(ok("hello there"), Command::Say(s("hello there")));
    assert_eq!(ok("  spaced   out  "), Command::Say(s("spaced   out")), "внутренние пробелы сохраняются, края обрезаются");
    assert_eq!(err("   "), ParseError::Empty, "пустая строка это Empty");
    assert_eq!(err(""), ParseError::Empty);
}

#[test]
fn double_slash_escapes_a_slash() {
    assert_eq!(ok("//msg is not a command"), Command::Say(s("/msg is not a command")));
    assert_eq!(ok("//"), Command::Say(s("/")));
}

#[test]
fn simple_commands() {
    assert_eq!(ok("/login ann_1"), Command::Login(s("ann_1")));
    assert_eq!(ok("/logout"), Command::Logout);
    assert_eq!(ok("/whoami"), Command::Whoami);
    assert_eq!(ok("/quit"), Command::Quit);
    assert_eq!(ok("   /whoami   "), Command::Whoami, "пробелы по краям терпим");
}

#[test]
fn unknown_commands() {
    assert_eq!(err("/dance now"), ParseError::Unknown(s("dance")));
    assert_eq!(err("/"), ParseError::Unknown(s("")), "одинокий слэш это неизвестная команда без имени");
    assert_eq!(err("/ msg ann hi"), ParseError::Unknown(s("")), "пробел сразу после слэша: имени команды нет");
    assert_eq!(err("/MSG ann hi"), ParseError::Unknown(s("MSG")), "имена команд чувствительны к регистру");
}

#[test]
fn msg_joins_the_rest_with_single_spaces() {
    assert_eq!(ok("/msg ann hi"), Command::Msg { to: s("ann"), text: s("hi") });
    assert_eq!(
        ok("/msg  bob_7   how   are  you"),
        Command::Msg { to: s("bob_7"), text: s("how are you") },
        "слова текста склеиваются одним пробелом"
    );
}

#[test]
fn msg_errors() {
    assert_eq!(err("/msg"), ParseError::MissingArg { command: "msg", what: "nick" });
    assert_eq!(err("/msg ann"), ParseError::MissingArg { command: "msg", what: "text" });
    assert_eq!(err("/msg Ann hi"), ParseError::InvalidNick(s("Ann")), "заглавные буквы в нике нельзя");
    assert_eq!(err("/msg an hi"), ParseError::InvalidNick(s("an")), "ник короче трёх");
    assert_eq!(err("/msg abcdefghijklmnopq hi"), ParseError::InvalidNick(s("abcdefghijklmnopq")), "ник длиннее шестнадцати");
    assert_eq!(err("/msg a-b hi"), ParseError::InvalidNick(s("a-b")), "дефис не разрешён");
    assert_eq!(err("/msg аня hi"), ParseError::InvalidNick(s("аня")), "кириллица не разрешена");
}

#[test]
fn nick_boundaries() {
    assert_eq!(ok("/login abc"), Command::Login(s("abc")));
    assert_eq!(ok("/login abcdefghijklmnop"), Command::Login(s("abcdefghijklmnop")));
    assert_eq!(ok("/login 0_9"), Command::Login(s("0_9")));
}

#[test]
fn history_with_optional_limit() {
    assert_eq!(ok("/history ann"), Command::History { with: s("ann"), limit: None });
    assert_eq!(ok("/history ann 20"), Command::History { with: s("ann"), limit: Some(20) });
    assert_eq!(ok("/history ann 1"), Command::History { with: s("ann"), limit: Some(1) });
    assert_eq!(ok("/history ann 100"), Command::History { with: s("ann"), limit: Some(100) });
    assert_eq!(err("/history"), ParseError::MissingArg { command: "history", what: "nick" });
    assert_eq!(err("/history ann x"), ParseError::BadNumber(s("x")));
    assert_eq!(err("/history ann -5"), ParseError::BadNumber(s("-5")));
    assert_eq!(err("/history ann 0"), ParseError::LimitOutOfRange(0), "ноль записей просить бессмысленно");
    assert_eq!(err("/history ann 101"), ParseError::LimitOutOfRange(101));
    assert_eq!(err("/history ann 5 6"), ParseError::TooManyArgs("history"));
}

#[test]
fn argument_shape_is_checked_before_content() {
    assert_eq!(err("/history BAD 5 6"), ParseError::TooManyArgs("history"), "сначала число аргументов");
    assert_eq!(err("/history BAD x"), ParseError::InvalidNick(s("BAD")), "потом ник, потом число");
    assert_eq!(err("/login a b c"), ParseError::TooManyArgs("login"));
    assert_eq!(err("/login"), ParseError::MissingArg { command: "login", what: "nick" });
    assert_eq!(err("/login A"), ParseError::InvalidNick(s("A")));
    assert_eq!(err("/logout now"), ParseError::TooManyArgs("logout"));
    assert_eq!(err("/whoami me"), ParseError::TooManyArgs("whoami"));
    assert_eq!(err("/quit please"), ParseError::TooManyArgs("quit"));
}

#[test]
fn odd_input_never_panics() {
    for line in ["\u{0}", "/\u{0}", "/msg \u{1F600} \u{1F600}", "\t\n", "/ msg ann hi", "/history ann 99999999999999999999999", "я", "/мsg ann hi"] {
        let _ = parse(line);
    }
}

fn login(user: &str) -> Session {
    let (session, reply) = Session::Anonymous.step(Command::Login(s(user))).expect("вход");
    assert_eq!(reply, Reply::Welcome(s(user)));
    session
}

fn rejected(session: Session, command: Command) -> Rejected {
    session.step(command).expect_err("команда должна быть отклонена")
}

#[test]
fn anonymous_can_only_login_whoami_quit() {
    assert_eq!(Session::Anonymous.step(Command::Whoami), Ok((Session::Anonymous, Reply::You(None))));
    assert_eq!(Session::Anonymous.step(Command::Quit), Ok((Session::Closed, Reply::Bye)));
    for cmd in [
        Command::Say(s("hi")),
        Command::Logout,
        Command::Msg { to: s("ann"), text: s("hi") },
        Command::History { with: s("ann"), limit: None },
    ] {
        assert_eq!(
            rejected(Session::Anonymous, cmd.clone()),
            Rejected { session: Session::Anonymous, reason: Reason::NotLoggedIn },
            "анонимному {cmd:?} недоступно"
        );
    }
}

#[test]
fn login_logout_cycle() {
    let session = login("ann");
    assert_eq!(session, Session::LoggedIn { user: s("ann") });
    assert_eq!(session.clone().step(Command::Whoami), Ok((session.clone(), Reply::You(Some(s("ann"))))));
    assert_eq!(session.clone().step(Command::Logout), Ok((Session::Anonymous, Reply::Goodbye)));
    assert_eq!(
        rejected(session.clone(), Command::Login(s("bob"))),
        Rejected { session, reason: Reason::AlreadyLoggedIn },
        "войти второй раз нельзя, а сессия возвращается нетронутой"
    );
}

#[test]
fn msg_starts_a_dialog_and_say_continues_it() {
    let session = login("ann");
    let (session, reply) = session.step(Command::Msg { to: s("bob"), text: s("hi") }).expect("сообщение");
    assert_eq!(session, Session::Chatting { user: s("ann"), peer: s("bob") });
    assert_eq!(reply, Reply::Sent { to: s("bob"), text: s("hi") });
    let (session, reply) = session.step(Command::Say(s("are you there"))).expect("реплика собеседнику");
    assert_eq!(reply, Reply::Sent { to: s("bob"), text: s("are you there") }, "обычный текст уходит текущему собеседнику");
    let (session, _) = session.step(Command::Msg { to: s("cat"), text: s("psst") }).expect("смена собеседника");
    assert_eq!(session, Session::Chatting { user: s("ann"), peer: s("cat") }, "новый /msg переключает диалог");
}

#[test]
fn say_without_a_peer_is_rejected() {
    let session = login("ann");
    assert_eq!(
        rejected(session.clone(), Command::Say(s("hello?"))),
        Rejected { session, reason: Reason::NoPeer },
        "без собеседника писать некому"
    );
}

#[test]
fn messaging_yourself_is_rejected_everywhere() {
    let logged = login("ann");
    assert_eq!(
        rejected(logged.clone(), Command::Msg { to: s("ann"), text: s("me") }),
        Rejected { session: logged, reason: Reason::SelfMessage }
    );
    let chatting = Session::Chatting { user: s("ann"), peer: s("bob") };
    assert_eq!(
        rejected(chatting.clone(), Command::Msg { to: s("ann"), text: s("me") }),
        Rejected { session: chatting, reason: Reason::SelfMessage },
        "в диалоге отказ тоже не должен рушить текущий диалог"
    );
}

#[test]
fn history_and_whoami_keep_the_state() {
    for session in [login("ann"), Session::Chatting { user: s("ann"), peer: s("bob") }] {
        let (after, reply) = session
            .clone()
            .step(Command::History { with: s("cat"), limit: Some(5) })
            .expect("история доступна");
        assert_eq!(after, session, "просмотр истории не меняет состояние");
        assert_eq!(reply, Reply::Show { with: s("cat"), limit: Some(5) });
        let (after, reply) = session.clone().step(Command::Whoami).expect("whoami доступен");
        assert_eq!(after, session);
        assert_eq!(reply, Reply::You(Some(s("ann"))));
    }
}

#[test]
fn chatting_can_logout_quit_but_not_relogin() {
    let chatting = Session::Chatting { user: s("ann"), peer: s("bob") };
    assert_eq!(chatting.clone().step(Command::Logout), Ok((Session::Anonymous, Reply::Goodbye)));
    assert_eq!(chatting.clone().step(Command::Quit), Ok((Session::Closed, Reply::Bye)));
    assert_eq!(
        rejected(chatting.clone(), Command::Login(s("cat"))),
        Rejected { session: chatting, reason: Reason::AlreadyLoggedIn }
    );
    assert_eq!(login("ann").step(Command::Quit), Ok((Session::Closed, Reply::Bye)));
}

#[test]
fn closed_session_rejects_everything() {
    for cmd in [
        Command::Say(s("hi")),
        Command::Login(s("ann")),
        Command::Logout,
        Command::Whoami,
        Command::Msg { to: s("ann"), text: s("hi") },
        Command::History { with: s("ann"), limit: None },
        Command::Quit,
    ] {
        assert_eq!(
            rejected(Session::Closed, cmd.clone()),
            Rejected { session: Session::Closed, reason: Reason::SessionClosed },
            "после /quit команда {cmd:?} не должна исполняться"
        );
    }
}

#[test]
fn a_whole_conversation_from_text_lines() {
    let script = ["/whoami", "/login ann", "/msg bob hi bob", "how are you", "/history bob 10", "/logout", "/quit"];
    let mut session = Session::Anonymous;
    let mut replies = Vec::new();
    for line in script {
        let command = parse(line).unwrap_or_else(|e| panic!("{line:?}: {e:?}"));
        let (next, reply) = session.step(command).unwrap_or_else(|r| panic!("{line:?} отклонена: {:?}", r.reason));
        session = next;
        replies.push(reply);
    }
    assert_eq!(session, Session::Closed);
    assert_eq!(
        replies,
        vec![
            Reply::You(None),
            Reply::Welcome(s("ann")),
            Reply::Sent { to: s("bob"), text: s("hi bob") },
            Reply::Sent { to: s("bob"), text: s("how are you") },
            Reply::Show { with: s("bob"), limit: Some(10) },
            Reply::Goodbye,
            Reply::Bye,
        ]
    );
}

#[test]
fn no_catch_all_arms_in_the_sources() {
    for file in ["src/command.rs", "src/session.rs"] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(file);
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("не удалось прочитать {file}: {e}"));
        let bad = text.lines().filter(|l| !l.trim_start().starts_with("//")).any(|l| l.contains("_ =>"));
        assert!(!bad, "в {file} есть ветка `_ =>`: перечисли случаи явно, чтобы новый вариант enum не прошёл незамеченным");
    }
}

#[test]
fn parser_uses_slice_patterns_and_let_else() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/command.rs");
    let text = std::fs::read_to_string(&path).expect("не удалось прочитать src/command.rs");
    assert!(text.contains("@ .."), "разбор слов должен идти срезовыми образцами вида `[\"msg\", nick, text @ ..]`");
    assert!(
        text.lines().any(|l| l.trim_start().starts_with("let ") && l.trim_end().ends_with("else {")),
        "хотя бы один ранний выход должен быть записан через let-else"
    );
}
