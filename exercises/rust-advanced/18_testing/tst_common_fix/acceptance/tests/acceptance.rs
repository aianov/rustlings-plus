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

fn tests_dir() -> PathBuf {
    project().join("crates/billing/tests")
}

#[test]
fn helpers_live_in_a_module_directory() {
    let tests = tests_dir();
    assert!(!tests.join("common.rs").exists(), "tests/common.rs cargo считает отдельным тестовым крейтом: перенеси помощников в tests/common/mod.rs");
    let module = fs::read_to_string(tests.join("common/mod.rs")).expect("нет tests/common/mod.rs");
    for helper in ["coffee_invoice", "empty_invoice", "with_discount"] {
        assert!(module.contains(&format!("pub fn {helper}")), "помощник `{helper}` должен быть pub, иначе тестовые файлы его не увидят");
    }
}

#[test]
fn test_files_keep_using_the_shared_module() {
    for name in ["totals.rs", "discounts.rs"] {
        let source = fs::read_to_string(tests_dir().join(name)).unwrap_or_else(|_| panic!("пропал файл tests/{name}"));
        assert!(source.contains("mod common;"), "tests/{name} должен подключать общий модуль `mod common;`, а не копировать помощников");
        assert!(!source.contains("fn coffee_invoice") && !source.contains("fn with_discount"), "tests/{name}: помощники должны жить в общем модуле, а не дублироваться");
        assert_eq!(source.matches("#[test]").count(), 3, "в tests/{name} должны остаться все три теста");
    }
}

#[test]
fn run_is_clean_and_has_no_empty_target() {
    let _guard = serial();
    let sandbox = sandbox("billing_clean");
    let out = cargo(&sandbox, &["test", "--workspace"]);
    let output = text(&out);
    assert!(out.status.success(), "cargo test упал:\n{output}");
    assert!(!output.contains("warning"), "в сборке тестов предупреждения: у общего модуля каждый файл использует не все функции\n{output}");
    assert!(!output.contains("tests/common"), "common не должен запускаться как тестовый крейт:\n{output}");
    let binaries: Vec<(String, usize)> = tests_per_binary(&output).into_iter().filter(|(name, _)| name.starts_with("tests/")).collect();
    assert_eq!(binaries.len(), 2, "должно быть ровно два интеграционных крейта (totals и discounts), нашёл {binaries:?}");
    assert_eq!(sum_field(&output, "passed"), 6, "должны пройти все шесть тестов\n{output}");
}
