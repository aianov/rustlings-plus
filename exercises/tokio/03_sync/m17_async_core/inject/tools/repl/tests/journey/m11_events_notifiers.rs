use std::io::{Read, Write};
use std::process::{Command as Process, ExitStatus, Stdio};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use contracts::Username;
use repl::{Session, Terminal};
use social_core::{Notification, Notifier, Notifiers, Response, UserId};

#[derive(Default)]
struct Recorder {
    got: Mutex<Vec<(UserId, Notification)>>,
}

impl Notifier for Recorder {
    fn notify(&self, user: UserId, notification: Notification) {
        self.got
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((user, notification));
    }
}

fn name(raw: &str) -> Username {
    Username::try_from(raw).expect("нормальный ник должен проходить проверку")
}

fn run(args: &[&str], script: &str) -> (ExitStatus, String, String) {
    let mut child = Process::new(env!("CARGO_BIN_EXE_repl"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
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
    let mut out = String::new();
    child
        .stdout
        .take()
        .expect("у процесса есть stdout")
        .read_to_string(&mut out)
        .expect("stdout читается");
    let mut err = String::new();
    child
        .stderr
        .take()
        .expect("у процесса есть stderr")
        .read_to_string(&mut err)
        .expect("stderr читается");
    (status, out, err)
}

#[test]
fn terminal_answers_with_responses() {
    let mut terminal = Terminal::new().expect("терминал должен запускаться");
    let script = [
        ("register alice", 200, "зарегистрирован @alice (id 1)"),
        ("register bob", 200, "зарегистрирован @bob (id 2)"),
        ("dance", 404, "неизвестная команда: dance"),
        ("/", 404, "неизвестная команда: /"),
        ("follow alice ghost", 404, "нет пользователя @ghost"),
        ("register ALICE", 422, "ник alice уже занят"),
        ("post alice", 400, "не хватает аргумента: текст поста"),
        ("/say привет", 401, "сначала войди: /login <ник>"),
        ("/login ghost", 404, "нет пользователя @ghost"),
        ("/login alice", 200, "ты @alice"),
        (
            "/login bob",
            409,
            "сессия @alice уже открыта, сначала /logout",
        ),
        (
            "/say привет",
            409,
            "диалог не открыт, начни его: /msg <ник> <текст>",
        ),
        ("/msg alice сам себе", 422, "нельзя написать самому себе"),
        ("/history bob 0", 400, "лимит истории от 1 до 100, а не 0"),
        ("/msg bob привет", 200, "@alice → @bob: привет"),
        ("/logout", 200, "до встречи, @alice"),
        ("inbox bob", 200, "@alice написал тебе"),
    ];
    for (line, status, body) in script {
        assert_eq!(
            terminal.handle(line),
            Response::new(status, body),
            "ответ терминала на {line:?}: команда идёт через шину, результат становится Response"
        );
    }
    assert_eq!(
        terminal.session(),
        Some(&Session::Anonymous),
        "сессия лежит в контексте шины и пережила все команды"
    );
}

#[test]
fn session_survives_in_the_context() {
    let mut terminal = Terminal::new().expect("терминал должен запускаться");
    for line in [
        "register alice",
        "register bob",
        "/login alice",
        "/msg bob привет",
    ] {
        assert!(terminal.handle(line).is_ok(), "строка {line:?} проходит");
    }
    assert_eq!(
        terminal.session(),
        Some(&Session::Chatting {
            user: name("alice"),
            peer: name("bob"),
        }),
        "после /msg в контексте лежит Chatting"
    );
}

#[test]
fn terminal_uses_notifiers_chosen_at_start() {
    let recorder = Arc::new(Recorder::default());
    let mut notifiers = Notifiers::new();
    notifiers.add(recorder.clone());
    let mut terminal = Terminal::with_notifiers(notifiers).expect("терминал должен запускаться");
    for line in [
        "register alice",
        "register bob",
        "follow bob alice",
        "/login alice",
        "/msg bob привет",
    ] {
        assert!(terminal.handle(line).is_ok(), "строка {line:?} проходит");
    }
    let got = recorder
        .got
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    assert_eq!(
        got,
        [
            (UserId(1), Notification::Followed { by: UserId(2) }),
            (UserId(2), Notification::Message { from: UserId(1) }),
        ],
        "Terminal::with_notifiers передаёт уведомители сети, и они слышат подписки и сообщения"
    );
}

#[test]
fn log_flag_adds_the_log_notifier() {
    let script = "register alice\nregister bob\nfollow bob alice\n/login bob\n/msg alice привет\ninbox alice\n";
    let stdout = "зарегистрирован @alice (id 1)\n\
                  зарегистрирован @bob (id 2)\n\
                  @bob подписан на @alice\n\
                  ты @bob\n\
                  @bob → @alice: привет\n\
                  @bob подписался на тебя\n\
                  @bob написал тебе\n";
    let (status, out, err) = run(&["--log"], script);
    assert!(status.success(), "repl --log завершается успешно");
    assert_eq!(out, stdout, "с --log ответы в stdout те же");
    assert_eq!(
        err, "[notify] id 1: подписка от id 2\n[notify] id 1: сообщение от id 2\n",
        "--log при запуске добавляет LogNotifier, который пишет уведомления в stderr"
    );
    let (status, out, err) = run(&[], script);
    assert!(status.success(), "repl без флагов завершается успешно");
    assert_eq!(out, stdout, "без --log ответы те же");
    assert_eq!(err, "", "без --log лог уведомлений не пишется");
}
