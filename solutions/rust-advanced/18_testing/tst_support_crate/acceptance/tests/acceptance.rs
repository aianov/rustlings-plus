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

fn tree(dir: &Path, edges: &str) -> String {
    let out = cargo(dir, &["tree", "-p", "shop-core", "-e", edges, "--prefix", "none", "-f", "{lib}"]);
    assert!(out.status.success(), "cargo tree упал:\n{}", text(&out));
    text(&out)
}

#[test]
fn release_build_of_the_core_has_no_fakes() {
    let _guard = serial();
    let sandbox = sandbox("support_normal");
    let normal = tree(&sandbox, "normal");
    assert!(!normal.contains("test_support"), "test-support попал в обычные зависимости shop-core, то есть в релиз:\n{normal}");
    assert!(normal.contains("contracts"), "контракты из зависимостей пропали:\n{normal}");
}

#[test]
fn tests_of_the_core_still_see_the_fakes() {
    let _guard = serial();
    let sandbox = sandbox("support_dev");
    let dev = tree(&sandbox, "dev");
    assert!(dev.contains("test_support"), "для тестов ядра test-support нужен как dev-dependency:\n{dev}");
    let out = cargo(&sandbox, &["test", "--workspace"]);
    let output = text(&out);
    assert!(out.status.success(), "cargo test --workspace упал:\n{output}");
    assert!(sum_field(&output, "passed") >= 3, "должны пройти три теста ядра\n{output}");
}

#[test]
fn fakes_exist_only_behind_the_feature() {
    let _guard = serial();
    let sandbox = sandbox("support_feature");
    write(&sandbox, "crates/test-support/examples/probe_fakes.rs", "use test_support::{FakeClock, SeqIds};\nfn main() {\n    let _ = (FakeClock::new(1), SeqIds::starting_at(1));\n}\n");
    write(&sandbox, "crates/test-support/examples/probe_plain.rs", "fn main() {\n    println!(\"{}\", test_support::unique_name(\"x\"));\n}\n");
    let without = cargo(&sandbox, &["check", "-p", "test-support", "--example", "probe_fakes"]);
    assert!(!without.status.success(), "без feature `test-util` фейки не должны существовать: спрячь их за `#[cfg(feature = \"test-util\")]`, а default оставь пустым");
    let with = cargo(&sandbox, &["check", "-p", "test-support", "--example", "probe_fakes", "--features", "test-util"]);
    assert!(with.status.success(), "с feature `test-util` фейки обязаны собираться:\n{}", text(&with));
    let plain = cargo(&sandbox, &["check", "-p", "test-support", "--example", "probe_plain"]);
    assert!(plain.status.success(), "unique_name нужен всегда, он не должен зависеть от feature:\n{}", text(&plain));
}

#[test]
fn feature_is_requested_by_the_core_tests_not_by_default() {
    let _guard = serial();
    let sandbox = sandbox("support_default");
    let out = cargo(&sandbox, &["tree", "-p", "test-support", "-e", "features", "--prefix", "none", "-f", "{lib} {f}"]);
    let output = text(&out);
    assert!(out.status.success(), "cargo tree упал:\n{output}");
    assert!(!output.contains("test-util"), "feature test-util включена по умолчанию (default = [..]): пусть её просят только тесты ядра\n{output}");
}
