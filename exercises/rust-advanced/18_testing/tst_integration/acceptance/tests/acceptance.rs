#![allow(dead_code)]
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn serial() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

fn project() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("папка acceptance лежит внутри проекта").to_path_buf()
}

fn shared_target() -> PathBuf {
    Path::new(env!("CARGO_TARGET_TMPDIR")).join("shared")
}

fn exec(mut cmd: Command) -> Output {
    use std::io::Read;
    use std::os::unix::process::CommandExt;
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).process_group(0);
    let mut child = cmd.spawn().expect("не удалось запустить программу");
    let mut out = child.stdout.take().expect("нет stdout");
    let mut err = child.stderr.take().expect("нет stderr");
    let read_out = std::thread::spawn(move || {
        let mut b = Vec::new();
        let _ = out.read_to_end(&mut b);
        b
    });
    let read_err = std::thread::spawn(move || {
        let mut b = Vec::new();
        let _ = err.read_to_end(&mut b);
        b
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(150);
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child.try_wait().expect("не удалось проверить процесс") {
            break status;
        }
        if std::time::Instant::now() > deadline {
            timed_out = true;
            let _ = Command::new("kill").args(["-KILL", &format!("-{}", child.id())]).status();
            let _ = child.kill();
            break child.wait().expect("не удалось дождаться процесса");
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    };
    let mut stderr = read_err.join().unwrap_or_default();
    if timed_out {
        stderr.extend_from_slice("\n[проверка остановила команду: она работала дольше 150 секунд]\n".as_bytes());
    }
    Output { status, stdout: read_out.join().unwrap_or_default(), stderr }
}

fn cargo(dir: &Path, args: &[&str]) -> Output {
    let mut cmd = Command::new("sh");
    cmd.args(["-c", "exec cargo \"$@\" 2>&1", "sh"])
        .args(args)
        .env("CARGO_TARGET_DIR", dir.join("target"))
        .env("CARGO_TERM_COLOR", "never")
        .env_remove("CLIPPY_CONF_DIR")
        .env_remove("RUSTFLAGS")
        .current_dir(dir);
    exec(cmd)
}

fn text(o: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
}

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("не удалось создать папку");
    for entry in fs::read_dir(from).expect("не удалось прочитать папку") {
        let entry = entry.expect("ошибка чтения папки");
        let name = entry.file_name();
        if name == "target" || name == "acceptance" || name == "Cargo.lock" {
            continue;
        }
        let (src, dst) = (entry.path(), to.join(&name));
        if src.is_dir() {
            copy_tree(&src, &dst);
        } else {
            fs::copy(&src, &dst).expect("не удалось скопировать файл");
        }
    }
}

fn sandbox(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("box_{name}"));
    let _ = fs::remove_dir_all(&dir);
    copy_tree(&project(), &dir);
    dir
}

fn edit(dir: &Path, rel: &str, from: &str, to: &str) {
    let path = dir.join(rel);
    let source = fs::read_to_string(&path).unwrap_or_else(|_| panic!("нет файла {rel}"));
    assert!(
        source.contains(from),
        "в {rel} не нашёлся рабочий фрагмент `{from}`: правь только тесты, рабочий код библиотеки менять нельзя"
    );
    fs::write(&path, source.replacen(from, to, 1)).expect("не удалось записать файл");
}

fn write(dir: &Path, rel: &str, content: &str) {
    let path = dir.join(rel);
    fs::create_dir_all(path.parent().expect("у файла есть папка")).expect("не удалось создать папку");
    fs::write(path, content).expect("не удалось записать файл");
}

fn sum_field(output: &str, word: &str) -> usize {
    output
        .lines()
        .filter(|l| l.starts_with("test result:"))
        .map(|l| {
            l.split(';')
                .find(|part| part.trim_end().ends_with(word))
                .and_then(|part| part.split_whitespace().rev().nth(1))
                .and_then(|n| n.parse::<usize>().ok())
                .unwrap_or(0)
        })
        .sum()
}

fn rs_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .map(|rd| rd.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "rs")).collect())
        .unwrap_or_default();
    files.sort();
    files
}

fn tests_per_binary(output: &str) -> Vec<(String, usize)> {
    let mut result = Vec::new();
    let mut current: Option<String> = None;
    for line in output.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("Running ") {
            current = Some(rest.split(' ').next().unwrap_or("").to_string());
        } else if let Some(rest) = t.strip_prefix("running ")
            && let Some(name) = current.take()
        {
            let n = rest.split(' ').next().and_then(|n| n.parse::<usize>().ok()).unwrap_or(0);
            result.push((name, n));
        }
    }
    result
}

const MUTANTS: [(&str, &str, &str); 9] = [
    ("номера начинаются с нуля", "next_id: 1 }", "next_id: 0 }"),
    ("пробелы внутри текста не схлопываются", "text.split_whitespace().collect::<Vec<_>>().join(\" \")", "text.trim().to_string()"),
    ("предел длины включает границу", "length > MAX_LEN", "length >= MAX_LEN"),
    ("mark_read ничего не отмечает", "message.read = true;", "message.read = message.read;"),
    ("clear_read чистит непрочитанные", "retain(|m| !m.read)", "retain(|m| m.read)"),
    ("поиск различает регистр", "m.text.to_lowercase().contains(&term)", "m.text.contains(&term)"),
    ("в NotFound не тот номер", "InboxError::NotFound(id))?", "InboxError::NotFound(0))?"),
    ("длина считается в байтах", "text.chars().count()", "text.len()"),
    ("отправитель теряется", "from: from.to_string()", "from: String::new()"),
];

fn tests_dir() -> PathBuf {
    project().join("crates/inbox/tests")
}

#[test]
fn layout_follows_the_rules() {
    let tests = tests_dir();
    assert!(!tests.join("common.rs").exists(), "tests/common.rs превратится в отдельный тестовый крейт `running 0 tests`: общий модуль кладут в tests/common/mod.rs");
    assert!(tests.join("common/mod.rs").exists(), "нет tests/common/mod.rs с общими помощниками");
    let files = rs_files(&tests);
    assert!(files.len() >= 2, "нужно минимум два файла интеграционных тестов в crates/inbox/tests/ (каждый файл это отдельный крейт)");
    for file in files {
        let source = fs::read_to_string(&file).expect("не удалось прочитать файл");
        assert!(source.contains("mod common;"), "{} не подключает общий модуль: `mod common;`", file.display());
    }
    let common = fs::read_to_string(tests.join("common/mod.rs")).expect("не удалось прочитать common/mod.rs");
    assert!(common.contains("pub fn"), "помощники в common/mod.rs должны быть pub, иначе тестовые файлы их не увидят");
}

#[test]
fn suite_is_green_clean_and_has_no_phantom_targets() {
    let _guard = serial();
    let sandbox = sandbox("inbox_clean");
    let out = cargo(&sandbox, &["test", "--workspace"]);
    let output = text(&out);
    assert!(out.status.success(), "cargo test упал:\n{output}");
    assert!(!output.contains("warning"), "в сборке тестов есть предупреждения (общий модуль, которым каждый файл пользуется не целиком, даёт dead_code):\n{output}");
    assert!(!output.contains("tests/common"), "common не должен запускаться как отдельный тестовый крейт:\n{output}");
    let integration: Vec<(String, usize)> = tests_per_binary(&output).into_iter().filter(|(name, _)| name.starts_with("tests/")).collect();
    assert!(integration.len() >= 2, "ожидаю минимум два интеграционных файла, нашёл {integration:?}");
    for (name, count) in &integration {
        assert!(*count >= 2, "в {name} меньше двух тестов");
    }
    assert!(sum_field(&output, "passed") >= 9, "всего тестов должно быть не меньше девяти\n{output}");
    assert!(sum_field(&output, "ignored") >= 1, "нет ни одного теста с #[ignore]: долгий тест на тысячи сообщений стоит отложить");
}

#[test]
fn ignored_tests_pass_when_asked_for() {
    let _guard = serial();
    let sandbox = sandbox("inbox_ignored");
    let out = cargo(&sandbox, &["test", "--workspace", "--", "--ignored"]);
    let output = text(&out);
    assert!(out.status.success(), "тесты с #[ignore] не проходят при запуске с --ignored:\n{output}");
    assert!(sum_field(&output, "passed") >= 1, "при --ignored не запустился ни один тест\n{output}");
}

#[test]
fn tests_notice_every_mutant() {
    let _guard = serial();
    for (index, (what, from, to)) in MUTANTS.iter().enumerate() {
        let sandbox = sandbox(&format!("inbox_mutant_{index}"));
        edit(&sandbox, "crates/inbox/src/lib.rs", from, to);
        let out = cargo(&sandbox, &["test", "--workspace"]);
        assert!(!out.status.success(), "тесты не заметили ошибку «{what}»: добавь проверку, которая на ней падает");
    }
}
