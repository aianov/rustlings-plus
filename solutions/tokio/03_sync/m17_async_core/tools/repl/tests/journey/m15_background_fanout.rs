use std::collections::BTreeSet;
use std::io::{Read, Write};
use std::process::{Command as Process, ExitStatus, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use contracts::{Outcome, PostId, PostView, Push, Response, UserId, UserView, decode};
use repl::Mode;
use social_core::Notifiers;

fn session(mode: Mode, script: &str) -> String {
    let script = script.to_string();
    let (done, result) = mpsc::channel();
    thread::Builder::new()
        .spawn(move || {
            let mut out = Vec::new();
            let finished = repl::run(mode, Notifiers::new(), script.as_bytes(), &mut out);
            let _ = done.send(finished.map(|()| out));
        })
        .expect("ОС должна дать тесту поток");
    let out = match result.recv_timeout(Duration::from_secs(60)) {
        Ok(out) => out.expect("сеанс завершается Ok"),
        Err(RecvTimeoutError::Timeout) => {
            panic!("сеанс не закончился за 60 с: похоже, терминал ждёт ядро вечно")
        }
        Err(RecvTimeoutError::Disconnected) => panic!("сеанс упал, причина в сообщении выше"),
    };
    String::from_utf8(out).expect("терминал пишет UTF-8")
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
fn followers_see_new_posts_in_the_terminal() {
    let script = "register alice\nregister bob\nregister carol\nfollow bob alice\nfollow carol alice\npost alice Привет #rust\ninbox bob\ninbox carol\nstats\ninbox alice\n";
    let out = session(Mode::Text, script);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(
        lines.get(5..),
        Some(
            &[
                "опубликован пост [1]",
                "@alice опубликовал пост [1]",
                "@alice опубликовал пост [1]",
                "постов: 1, уведомлений: 4",
                "@bob подписался на тебя",
                "@carol подписался на тебя",
            ][..]
        ),
        "подписчики видят новый пост в inbox, stats считает пост и четыре уведомления, а пришло:\n{out}"
    );
}

#[test]
fn json_mode_pushes_new_posts_right_after_the_answer() {
    let script = [
        r#"{"id": 1, "service": "users", "method": "users.register", "data": {"username": "alice"}}"#,
        r#"{"id": 2, "service": "users", "method": "users.register", "data": {"username": "bob"}}"#,
        r#"{"id": 3, "service": "users", "method": "users.register", "data": {"username": "carol"}}"#,
        r#"{"id": 4, "service": "users", "method": "users.follow", "data": {"user": "u_2", "target": "u_1"}}"#,
        r#"{"id": 5, "service": "users", "method": "users.follow", "data": {"user": "u_3", "target": "u_1"}}"#,
        r#"{"id": 6, "service": "posts", "method": "posts.create", "data": {"user": "u_1", "text": "Привет"}}"#,
        r#"{"id": 7, "service": "users", "method": "users.find", "data": {"username": "alice"}}"#,
    ]
    .join("\n");
    let out = session(Mode::Json, &script);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(
        lines.len(),
        11,
        "7 ответов, 2 push о подписках и 2 push о новом посте, а пришло:\n{out}"
    );
    assert_eq!(
        decode::<Response<PostView>>(lines[7]).map(|response| response.id),
        Ok(Some(6)),
        "после push о подписке carol идёт ответ на posts.create, строка {:?}",
        lines[7]
    );
    let pushes: BTreeSet<String> = lines[8..10]
        .iter()
        .map(|line| format!("{:?}", decode::<Push>(line)))
        .collect();
    let expected: BTreeSet<String> = [UserId(2), UserId(3)]
        .into_iter()
        .map(|to| {
            let push = Push::NewPost {
                to,
                author: UserId(1),
                post: PostId(1),
            };
            format!("{:?}", Ok::<Push, contracts::Error>(push))
        })
        .collect();
    assert_eq!(
        pushes, expected,
        "сразу за ответом на posts.create идут push о новом посте bob и carol (порядок любой): \
         JSON-режим дожидается рассылки, прежде чем отдать push, а пришло:\n{out}"
    );
    assert_eq!(
        decode::<Response<UserView>>(lines[10]).map(|response| response.outcome),
        Ok(Outcome::Ok(UserView {
            id: UserId(1),
            username: String::from("alice"),
        })),
        "ответ на следующую строку идёт уже после push, строка {:?}",
        lines[10]
    );
}

#[test]
fn log_is_complete_when_the_terminal_exits() {
    let mut script = String::from("register alice\n");
    for i in 0..20 {
        script.push_str(&format!("register fan{i}\nfollow fan{i} alice\n"));
    }
    script.push_str("post alice Всем привет\n");
    let (status, _, err) = run(&["--log"], &script);
    assert!(status.success(), "repl --log завершается успешно");
    let notified: BTreeSet<&str> = err
        .lines()
        .filter(|line| line.ends_with(": новый пост [1] от id 1"))
        .collect();
    let expected: BTreeSet<String> = (2..=21)
        .map(|id| format!("[notify] id {id}: новый пост [1] от id 1"))
        .collect();
    assert_eq!(
        notified,
        expected
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<&str>>(),
        "при выходе терминал дожидается рассылки: в логе все 20 уведомлений о новом посте, \
         а пришло:\n{err}"
    );
}
