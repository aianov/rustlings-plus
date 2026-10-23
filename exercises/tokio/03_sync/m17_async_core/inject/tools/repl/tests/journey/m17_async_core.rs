use std::fs;
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command as Process, ExitStatus, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use repl::{Mode, Terminal};
use social_core::Notifiers;

fn session(script: &str) -> String {
    let script = script.to_string();
    let (done, result) = mpsc::channel();
    thread::Builder::new()
        .spawn(move || {
            let mut out = Vec::new();
            let finished = repl::run(Mode::Text, Notifiers::new(), script.as_bytes(), &mut out);
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

fn run(script: &str) -> (ExitStatus, String) {
    let mut child = Process::new(env!("CARGO_BIN_EXE_repl"))
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
    let mut out = String::new();
    child
        .stdout
        .take()
        .expect("у процесса есть stdout")
        .read_to_string(&mut out)
        .expect("stdout читается");
    (status, out)
}

fn seed(name: &str, text: &str) -> String {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("m17-repl");
    fs::create_dir_all(&dir).expect("временная папка создаётся");
    let path = dir.join(name);
    fs::write(&path, text).expect("файл импорта пишется");
    path.display().to_string()
}

const SEED: &str = "register alice\nregister bob\nregister carol\nfollow bob alice\nfollow carol alice\npost alice Первый #rust\npost alice Второй\n";

#[test]
fn import_command_loads_the_seed_file() {
    let path = seed("seed.txt", SEED);
    let out = session(&format!("import {path}\nfeed bob\nstats\ninbox carol\n"));
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(
        lines.get(..4),
        Some(
            &[
                "импортировано: пользователей 3, подписок 2, постов 2",
                "@alice [2] Второй (лайков: 0)",
                "@alice [1] Первый #rust (лайков: 0)",
                "постов: 2, уведомлений: 6",
            ][..]
        ),
        "import загружает стартовые данные, посты одного автора идут в порядке файла, а дальше \
         с ними работают обычные команды: {lines:?}"
    );
    let mut inbox = lines.get(4..).map(<[&str]>::to_vec).unwrap_or_default();
    inbox.sort_unstable();
    assert_eq!(
        inbox,
        ["@alice опубликовал пост [1]", "@alice опубликовал пост [2]"],
        "подписчица carol узнала об обоих импортированных постах"
    );
}

#[test]
fn import_errors_do_not_stop_the_terminal() {
    let broken = seed("broken.txt", "register zed\nregister zed\n");
    let out = session(&format!(
        "import {broken}\nimport\nregister amy\nimport нет/такого.txt\n"
    ));
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(
        lines.first().copied(),
        Some("ошибка: импорт остановлен: строка 2: ник zed уже занят"),
        "ошибка импорта печатается всей цепочкой причин"
    );
    assert_eq!(
        lines.get(1).copied(),
        Some("ошибка: не хватает аргумента: путь к файлу"),
        "import без пути это ошибка разбора"
    );
    assert!(
        lines
            .get(2)
            .is_some_and(|line| line.starts_with("зарегистрирован @amy")),
        "после ошибок терминал работает дальше: {lines:?}"
    );
    assert!(
        lines
            .get(3)
            .is_some_and(|line| line.starts_with("ошибка: импорт остановлен: не прочитать")),
        "нет файла: ошибка чтения, а не паника: {lines:?}"
    );
}

#[test]
fn terminal_brings_its_own_runtime() {
    let mut terminal = Terminal::new().expect("терминал запускает рантайм сам");
    assert_eq!(
        terminal.handle("register alice").body,
        "зарегистрирован @alice (id 1)",
        "Terminal::new собирает рантайм своим Builder: вызывающему не нужен ни #[tokio::main], ни Handle"
    );
    let path = seed("terminal.txt", "register bob\nfollow bob alice\n");
    let reply = terminal.handle(&format!("import {path}"));
    assert_eq!(
        (reply.status, reply.body.as_str()),
        (200, "импортировано: пользователей 1, подписок 1, постов 0"),
        "импорт идёт через шину терминала"
    );
}

#[test]
fn binary_imports_and_exits_cleanly() {
    let path = seed("binary.txt", SEED);
    let (status, out) = run(&format!("import {path}\nposts bob alice 1\n"));
    assert!(status.success(), "терминал завершается кодом 0: {status}");
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(
        lines.first().copied(),
        Some("импортировано: пользователей 3, подписок 2, постов 2")
    );
    assert!(
        lines
            .get(1)
            .is_some_and(|line| line.starts_with("@alice [2] Второй")),
        "посты alice видны bob страницей автора: {lines:?}"
    );
}
