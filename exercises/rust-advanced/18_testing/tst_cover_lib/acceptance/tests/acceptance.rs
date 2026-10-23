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

const MUTANTS: [(&str, &str, &str); 12] = [
    ("длина считается в байтах", "let chars = text.chars().count();", "let chars = text.len();"),
    ("предел длины включает границу", "chars > MAX_CHARS", "chars >= MAX_CHARS"),
    ("пауза заканчивается на секунду позже", "passed < COOLDOWN_SECS", "passed <= COOLDOWN_SECS"),
    ("оставшееся время считается неверно", "wait: COOLDOWN_SECS - passed", "wait: COOLDOWN_SECS"),
    ("курсор страницы не исключается", "p.id < b", "p.id <= b"),
    ("порядок ленты перевёрнут", "b.id.cmp(&a.id)", "a.id.cmp(&b.id)"),
    ("фильтр автора различает регистр", "p.author.to_lowercase() == a.to_lowercase()", "p.author == a"),
    ("авторы в сводке считаются по постам", "authors.len())", "posts.len())"),
    ("управляющие символы не вычищаются", ".filter(|c| !c.is_control() || *c == '\\n')", ".filter(|_| true)"),
    ("пауза общая для всех авторов", ".filter(|p| p.author == author)", ".filter(|_| true)"),
    ("автор не обрезается", "let author = author.trim();", "let author = author;"),
    ("пауза от первого поста, а не от последнего", ".map(|p| p.at).max()", ".map(|p| p.at).min()"),
];

const LIB: &str = "crates/feedlib/src/lib.rs";

fn tests_dir() -> PathBuf {
    project().join("crates/feedlib/tests")
}

fn lib_tests_part() -> String {
    let source = fs::read_to_string(project().join(LIB)).expect("нет src/lib.rs");
    source.split_once("#[cfg(test)]").map(|(_, tests)| tests.to_string()).unwrap_or_default()
}

#[test]
fn every_kind_of_test_is_present() {
    let unit = lib_tests_part();
    assert!(unit.matches("#[test]").count() >= 4, "в `#[cfg(test)] mod tests` внутри src/lib.rs нужно не меньше четырёх юнит-тестов");
    assert!(unit.contains("clean("), "приватную функцию `clean` можно проверить только юнит-тестом внутри крейта: проверь её");
    assert!(unit.contains("impl Clock for"), "юнит-тестам нужны свои часы: сделай в модуле tests подставной `impl Clock for ...`");
    let tests = tests_dir();
    assert!(!tests.join("common.rs").exists(), "общий модуль кладут в tests/common/mod.rs, а не в tests/common.rs");
    let common = fs::read_to_string(tests.join("common/mod.rs")).expect("нет tests/common/mod.rs с общими помощниками");
    assert!(common.contains("impl Clock for"), "в tests/common/mod.rs должны жить подставные часы (`impl Clock for ...`)");
    let files = rs_files(&tests);
    assert!(files.len() >= 2, "нужно минимум два файла интеграционных тестов");
    for file in files {
        let source = fs::read_to_string(&file).expect("не удалось прочитать файл");
        assert!(source.contains("mod common;"), "{} не использует общий модуль", file.display());
        assert!(source.matches("#[test]").count() >= 2, "в {} меньше двух тестов", file.display());
    }
}

#[test]
fn tests_never_touch_real_time() {
    let mut sources = vec![lib_tests_part()];
    for file in rs_files(&tests_dir()).into_iter().chain(rs_files(&tests_dir().join("common"))) {
        sources.push(fs::read_to_string(file).expect("не удалось прочитать файл"));
    }
    sources.push(fs::read_to_string(tests_dir().join("common/mod.rs")).expect("нет tests/common/mod.rs"));
    for source in sources {
        for banned in ["SystemTime", "Instant::now", "thread::sleep", "sleep("] {
            assert!(!source.contains(banned), "в тестах нашлось `{banned}`: время подставляют через Clock, а не ждут и не читают настоящее");
        }
    }
}

#[test]
fn suite_is_green_clean_and_independent_of_scheduling() {
    let _guard = serial();
    let sandbox = sandbox("feed_clean");
    let out = cargo(&sandbox, &["test", "--workspace"]);
    let output = text(&out);
    assert!(out.status.success(), "cargo test упал:\n{output}");
    assert!(!output.contains("warning"), "в сборке есть предупреждения:\n{output}");
    assert!(!output.contains("tests/common"), "common не должен запускаться как тестовый крейт:\n{output}");
    let docs = cargo(&sandbox, &["test", "--workspace", "--doc"]);
    let doc_output = text(&docs);
    assert!(docs.status.success(), "документационные тесты упали:\n{doc_output}");
    assert!(sum_field(&doc_output, "passed") >= 2, "нужно минимум два документационных примера с проверками (publish и page)\n{doc_output}");
    for threads in ["1", "8"] {
        let out = cargo(&sandbox, &["test", "--workspace", "--", &format!("--test-threads={threads}")]);
        assert!(out.status.success(), "тесты зависят от порядка или параллельности (потоков: {threads}):\n{}", text(&out));
    }
}

#[test]
fn tests_notice_every_mutant() {
    let _guard = serial();
    for (index, (what, from, to)) in MUTANTS.iter().enumerate() {
        let sandbox = sandbox(&format!("feed_mutant_{index}"));
        edit(&sandbox, LIB, from, to);
        let out = cargo(&sandbox, &["test", "--workspace"]);
        assert!(!out.status.success(), "тесты не заметили ошибку «{what}»: добавь проверку, которая на ней падает");
    }
}
