use std::io::{Read, Write};
use std::process::{Command as Process, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use contracts::{ChatCommand, Command, ParseError, Username};
use repl::{Action, Session, SessionError, Terminal, TerminalError};
use social_core::{CommandError, DomainError, IntoResponse};

fn name(raw: &str) -> Username {
    Username::try_from(raw).expect("нормальный ник должен проходить проверку")
}

fn logged(user: &str) -> Session {
    Session::LoggedIn { user: name(user) }
}

fn chatting(user: &str, peer: &str) -> Session {
    Session::Chatting {
        user: name(user),
        peer: name(peer),
    }
}

fn msg(to: &str, text: &str) -> ChatCommand {
    ChatCommand::Msg {
        to: name(to),
        text: String::from(text),
    }
}

fn say(text: &str) -> ChatCommand {
    ChatCommand::Say(String::from(text))
}

fn history(with: &str, limit: usize) -> ChatCommand {
    ChatCommand::History {
        with: name(with),
        limit,
    }
}

fn send(from: &str, to: &str, text: &str) -> Action {
    Action::Run(Command::SendMessage {
        from: name(from),
        to: name(to),
        text: String::from(text),
    })
}

fn show(who: &str, with: &str, limit: usize) -> Action {
    Action::Run(Command::History {
        who: name(who),
        with: name(with),
        limit,
    })
}

fn terminal() -> Terminal {
    let mut terminal = Terminal::new().expect("терминал должен запускаться");
    for line in ["register alice", "register bob", "register carol"] {
        assert!(
            terminal.handle(line).is_ok(),
            "сетевые команды работают в терминале как раньше"
        );
    }
    terminal
}

fn ok(terminal: &mut Terminal, line: &str) -> String {
    let response = terminal.handle(line);
    if !response.is_ok() {
        panic!(
            "строка {line:?} должна пройти, а дала ошибку {}: {}",
            response.status, response.body
        );
    }
    response.body
}

#[test]
fn anonymous_can_only_log_in_or_ask() {
    let session = Session::Anonymous;
    assert_eq!(
        session.step(ChatCommand::Login(name("alice"))),
        Ok((logged("alice"), Action::Login(name("alice")))),
        "Anonymous + Login(u): LoggedIn {{ u }} и действие Login(u)"
    );
    assert_eq!(
        session.step(ChatCommand::Whoami),
        Ok((Session::Anonymous, Action::Whoami)),
        "Anonymous + Whoami: состояние то же, действие Whoami"
    );
    for command in [
        ChatCommand::Logout,
        msg("bob", "привет"),
        say("привет"),
        history("bob", 5),
    ] {
        assert_eq!(
            session.step(command.clone()),
            Err(SessionError::NotLoggedIn),
            "аноним не может {command:?}: отказ NotLoggedIn"
        );
    }
}

#[test]
fn logged_in_user_opens_dialogs() {
    let session = logged("alice");
    let cases = [
        (
            ChatCommand::Login(name("bob")),
            Err(SessionError::AlreadyLoggedIn(name("alice"))),
        ),
        (
            ChatCommand::Logout,
            Ok((Session::Anonymous, Action::Logout(name("alice")))),
        ),
        (ChatCommand::Whoami, Ok((logged("alice"), Action::Whoami))),
        (
            msg("bob", "привет"),
            Ok((chatting("alice", "bob"), send("alice", "bob", "привет"))),
        ),
        (say("привет"), Err(SessionError::NoPeer)),
        (
            history("bob", 5),
            Ok((logged("alice"), show("alice", "bob", 5))),
        ),
    ];
    for (command, expected) in cases {
        assert_eq!(
            session.step(command.clone()),
            expected.clone(),
            "LoggedIn {{ alice }} + {command:?} должно дать {expected:?}"
        );
    }
}

#[test]
fn chatting_user_talks_to_the_peer() {
    let session = chatting("alice", "bob");
    let cases = [
        (
            say("как дела?"),
            Ok((chatting("alice", "bob"), send("alice", "bob", "как дела?"))),
        ),
        (
            msg("carol", "привет"),
            Ok((chatting("alice", "carol"), send("alice", "carol", "привет"))),
        ),
        (
            ChatCommand::Login(name("carol")),
            Err(SessionError::AlreadyLoggedIn(name("alice"))),
        ),
        (
            ChatCommand::Logout,
            Ok((Session::Anonymous, Action::Logout(name("alice")))),
        ),
        (
            ChatCommand::Whoami,
            Ok((chatting("alice", "bob"), Action::Whoami)),
        ),
        (
            history("carol", 5),
            Ok((chatting("alice", "bob"), show("alice", "carol", 5))),
        ),
    ];
    for (command, expected) in cases {
        assert_eq!(
            session.step(command.clone()),
            expected.clone(),
            "Chatting {{ alice, bob }} + {command:?} должно дать {expected:?}"
        );
    }
}

#[test]
fn session_reads_like_sentences() {
    let texts = [
        (Session::Anonymous.to_string(), "вход не выполнен"),
        (logged("alice").to_string(), "ты @alice"),
        (
            chatting("alice", "bob").to_string(),
            "ты @alice, диалог с @bob",
        ),
        (
            SessionError::NotLoggedIn.to_string(),
            "сначала войди: /login <ник>",
        ),
        (
            SessionError::AlreadyLoggedIn(name("alice")).to_string(),
            "сессия @alice уже открыта, сначала /logout",
        ),
        (
            SessionError::NoPeer.to_string(),
            "диалог не открыт, начни его: /msg <ник> <текст>",
        ),
    ];
    for (shown, expected) in texts {
        assert_eq!(shown, expected, "Display сессии и её ошибок");
    }
    let boxed: Box<dyn std::error::Error> = Box::new(SessionError::NoPeer);
    assert_eq!(
        boxed.to_string(),
        "диалог не открыт, начни его: /msg <ник> <текст>",
        "SessionError реализует std::error::Error"
    );
}

#[test]
fn terminal_runs_a_conversation() {
    let mut terminal = terminal();
    let script = [
        ("/whoami", "вход не выполнен"),
        ("/login Alice", "ты @alice"),
        ("/msg bob привет", "@alice → @bob: привет"),
        ("/say как   дела?", "@alice → @bob: как дела?"),
        ("/whoami", "ты @alice, диалог с @bob"),
        ("/logout", "до встречи, @alice"),
        ("/login bob", "ты @bob"),
        ("/history alice", "@alice: привет\n@alice: как дела?"),
        ("/msg alice норм", "@bob → @alice: норм"),
        ("/history alice 2", "@alice: как дела?\n@bob: норм"),
        ("/history carol", "переписки пока нет"),
        ("post bob Пишу из чата", "опубликован пост [1]"),
        ("/whoami", "ты @bob, диалог с @alice"),
    ];
    for (line, reply) in script {
        assert_eq!(ok(&mut terminal, line), reply, "ответ на {line:?}");
    }
    assert_eq!(
        terminal.session(),
        Some(&chatting("bob", "alice")),
        "после разговора терминал помнит, кто вошёл и с кем диалог"
    );
}

#[test]
fn rejected_events_keep_the_session() {
    let mut terminal = terminal();
    assert_eq!(
        terminal.handle("/msg bob привет"),
        TerminalError::Session(SessionError::NotLoggedIn).into_response(),
        "аноним не пишет сообщений"
    );
    assert_eq!(
        terminal.handle("/login ghost"),
        TerminalError::Command(CommandError::UnknownName(String::from("ghost"))).into_response(),
        "войти можно только под зарегистрированным ником"
    );
    assert_eq!(
        terminal.session(),
        Some(&Session::Anonymous),
        "неудачный вход сессию не открывает"
    );
    ok(&mut terminal, "/login alice");
    let refused = [
        ("/say привет", TerminalError::Session(SessionError::NoPeer)),
        (
            "/login bob",
            TerminalError::Session(SessionError::AlreadyLoggedIn(name("alice"))),
        ),
        (
            "/msg alice сам себе",
            TerminalError::Command(CommandError::Domain(DomainError::SelfMessage)),
        ),
        (
            "/msg ghost привет",
            TerminalError::Command(CommandError::UnknownName(String::from("ghost"))),
        ),
    ];
    for (line, error) in refused {
        assert_eq!(
            terminal.handle(line),
            error.clone().into_response(),
            "строка {line:?} должна дать {error:?}"
        );
        assert_eq!(
            terminal.session(),
            Some(&logged("alice")),
            "после отказа на {line:?} сессия осталась прежней: диалог не открылся"
        );
    }
    ok(&mut terminal, "/msg bob привет");
    assert_eq!(
        terminal.handle("/msg ghost эй"),
        TerminalError::Command(CommandError::UnknownName(String::from("ghost"))).into_response(),
        "сообщение незнакомцу не уходит"
    );
    assert_eq!(
        terminal.handle("/dance"),
        TerminalError::Command(CommandError::Parse(ParseError::UnknownCommand(
            String::from("/dance")
        )))
        .into_response(),
        "неизвестная команда сессии это ошибка разбора"
    );
    assert_eq!(
        terminal.session(),
        Some(&chatting("alice", "bob")),
        "неудачный /msg не переключает диалог"
    );
    assert_eq!(
        TerminalError::Session(SessionError::NotLoggedIn).to_string(),
        "сначала войди: /login <ник>",
        "TerminalError печатает ошибку, которую несёт"
    );
}

#[test]
fn terminal_binary_keeps_the_session_between_lines() {
    let script = "register alice\nregister bob\n/msg bob привет\n/login alice\n/msg bob привет\n\n/say как жизнь?\n/whoami\n/logout\n/login bob\n/history alice\n/login alice\n/logout\n/login bob\n/say ну?\n/dance\n";
    let expected = "зарегистрирован @alice (id 1)\n\
                    зарегистрирован @bob (id 2)\n\
                    ошибка: сначала войди: /login <ник>\n\
                    ты @alice\n\
                    @alice → @bob: привет\n\
                    @alice → @bob: как жизнь?\n\
                    ты @alice, диалог с @bob\n\
                    до встречи, @alice\n\
                    ты @bob\n\
                    @alice: привет\n\
                    @alice: как жизнь?\n\
                    ошибка: сессия @bob уже открыта, сначала /logout\n\
                    до встречи, @bob\n\
                    ты @bob\n\
                    ошибка: диалог не открыт, начни его: /msg <ник> <текст>\n\
                    ошибка: неизвестная команда: /dance\n";
    let mut child = Process::new(env!("CARGO_BIN_EXE_repl"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("бинарник repl (tools/repl) должен запускаться");
    child
        .stdin
        .take()
        .expect("у процесса есть stdin")
        .write_all(script.as_bytes())
        .expect("скрипт пишется в stdin");
    let deadline = Instant::now() + Duration::from_secs(180);
    let status = loop {
        match child.try_wait().expect("состояние процесса читается") {
            Some(status) => break status,
            None if Instant::now() > deadline => {
                let _ = child.kill();
                panic!("терминал не завершился за 180 секунд после конца stdin");
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
    assert!(
        status.success(),
        "терминал должен завершаться успешно, когда stdin кончился"
    );
    assert_eq!(
        output, expected,
        "одна сессия на весь запуск терминала: состояние переживает строки, отказ печатается как `ошибка: ...`"
    );
}
