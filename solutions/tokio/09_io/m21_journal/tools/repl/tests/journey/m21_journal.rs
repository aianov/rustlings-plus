use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command as Process, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use repl::{JOURNAL_ENV, Mode};
use social_core::Notifiers;

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("m21-repl")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    dir.join("journal.ndjson")
}

fn session(journal: &Path, script: &str) -> String {
    let (script, journal) = (script.to_string(), journal.to_path_buf());
    let (done, result) = mpsc::channel();
    thread::Builder::new()
        .spawn(move || {
            let mut out = Vec::new();
            let input = script.as_bytes();
            let finished = repl::run_at(
                Mode::Text,
                Notifiers::new(),
                Some(&journal),
                input,
                &mut out,
            );
            let _ = done.send(finished.map(|()| out));
        })
        .expect("ОС должна дать тесту поток");
    let out = match result.recv_timeout(Duration::from_secs(60)) {
        Ok(out) => {
            out.unwrap_or_else(|e| panic!("сеанс с журналом завершается Ok, а пришло {e:?}"))
        }
        Err(RecvTimeoutError::Timeout) => panic!("сеанс не закончился за 60 с"),
        Err(RecvTimeoutError::Disconnected) => panic!("сеанс упал, причина в сообщении выше"),
    };
    String::from_utf8(out).expect("терминал пишет UTF-8")
}

#[test]
fn terminal_remembers_everything_between_sessions() {
    let journal = scratch("sessions");
    session(
        &journal,
        "register alice\nregister bob\nfollow bob alice\npost alice Журнал помнит #rust\nlike bob 1\n",
    );
    assert!(
        journal.exists(),
        "после сеанса журнал лежит по указанному пути, папки создаются сами"
    );
    let out = session(&journal, "feed bob\nregister alice\nregister carol\n");
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(
        lines.first().copied(),
        Some("@alice [1] Журнал помнит #rust (лайков: 1)"),
        "второй сеанс начинается с проигрывания журнала: лента bob и лайк на месте\n{out}"
    );
    assert!(
        lines.get(1).is_some_and(|line| line.starts_with("ошибка:")),
        "alice уже есть в журнале, повторная регистрация это отказ\n{out}"
    );
    assert_eq!(
        lines.get(2).copied(),
        Some("зарегистрирован @carol (id 3)"),
        "id продолжаются после восстановления\n{out}"
    );
}

fn run(journal: &Path, script: &str) -> String {
    let mut child = Process::new(env!("CARGO_BIN_EXE_repl"))
        .env(JOURNAL_ENV, journal)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
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
    assert!(status.success(), "терминал с журналом завершается кодом 0");
    let mut out = String::new();
    child
        .stdout
        .take()
        .expect("у процесса есть stdout")
        .read_to_string(&mut out)
        .expect("stdout читается");
    out
}

#[test]
fn journal_path_comes_from_the_environment() {
    let journal = scratch("env");
    run(&journal, "register alice\npost alice Из окружения\n");
    let out = run(&journal, "posts alice alice\n");
    assert!(
        out.contains("Из окружения"),
        "путь журнала терминал берёт из переменной {JOURNAL_ENV}: второй запуск видит пост первого\n{out}"
    );
}
