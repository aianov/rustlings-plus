#![allow(dead_code)]
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn serial() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

fn project() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("папка acceptance лежит внутри проекта")
        .to_path_buf()
}

fn shared_target() -> PathBuf {
    Path::new(env!("CARGO_TARGET_TMPDIR")).join("shared")
}

fn exec(mut cmd: Command, stdin: Option<&[u8]>) -> Output {
    use std::io::{Read, Write};
    use std::os::unix::process::CommandExt;
    cmd.stdin(if stdin.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    let mut child = cmd.spawn().expect("не удалось запустить программу");
    if let (Some(data), Some(mut pipe)) = (stdin, child.stdin.take()) {
        let _ = pipe.write_all(data);
    }
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
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
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
        stderr.extend_from_slice("
[тест остановил команду: она работала дольше 120 секунд]
".as_bytes());
    }
    Output { status, stdout: read_out.join().unwrap_or_default(), stderr }
}

fn target_for(dir: &Path) -> PathBuf {
    if dir.starts_with(project()) { shared_target() } else { dir.join("target") }
}

fn run_env(dir: &Path, program: &str, args: &[&str], envs: &[(&str, &str)]) -> Output {
    let mut cmd = Command::new(program);
    cmd.args(args)
        .envs(envs.iter().copied())
        .env("CARGO_TARGET_DIR", target_for(dir))
        .env_remove("CLIPPY_CONF_DIR")
        .current_dir(dir);
    exec(cmd, None)
}

fn run(dir: &Path, program: &str, args: &[&str]) -> Output {
    run_env(dir, program, args, &[])
}

fn make(dir: &Path, args: &[&str]) -> Output {
    run(dir, "make", args)
}

fn cargo(dir: &Path, args: &[&str]) -> Output {
    run(dir, "cargo", args)
}

fn cargo_make(dir: &Path, args: &[&str]) -> Output {
    let mut all = vec!["make"];
    all.extend_from_slice(args);
    run(dir, "cargo", &all)
}

fn text(o: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
}

fn ok(o: &Output, what: &str) -> String {
    assert!(o.status.success(), "{what} завершилась с ошибкой:\n{}", text(o));
    text(o)
}

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("не удалось создать папку");
    for entry in fs::read_dir(from).expect("не удалось прочитать папку") {
        let entry = entry.expect("ошибка чтения папки");
        let name = entry.file_name();
        if name == "target" || name == "acceptance" {
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

fn commands(output: &str) -> Vec<String> {
    output
        .lines()
        .filter_map(|l| l.split_once("Execute Command: ").map(|(_, c)| c.replace("\"", "")))
        .collect()
}

fn program_output(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout)
        .lines()
        .filter(|l| !l.starts_with("[cargo-make]"))
        .last()
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn index_of(haystack: &str, needle: &str) -> usize {
    haystack.find(needle).unwrap_or_else(|| panic!("в выводе нет `{needle}`:\n{haystack}"))
}

fn read_toml(path: &Path) -> toml::Table {
    let t = fs::read_to_string(path).unwrap_or_else(|_| panic!("нет файла {}", path.display()));
    t.parse::<toml::Table>().unwrap_or_else(|e| panic!("{} не разбирается как TOML: {e}", path.display()))
}

fn spoil_formatting(dir: &Path, rel: &str) {
    let path = dir.join(rel);
    let mut t = fs::read_to_string(&path).expect("нет файла");
    t.push_str("\nfn   badly_formatted ( ) {  let   x=1 ;  let _=x; }\n");
    fs::write(&path, t).expect("не удалось записать файл");
}

fn spoil_clippy(dir: &Path, rel: &str) {
    let path = dir.join(rel);
    let mut t = fs::read_to_string(&path).expect("нет файла");
    t.push_str("\npub fn clippy_bait(x: bool) -> bool {\n    if x == true { true } else { false }\n}\n");
    fs::write(&path, t).expect("не удалось записать файл");
}

fn stdout(o: &Output) -> String {
    program_output(o)
}

#[test]
fn check_runs_exactly_three_steps_in_order() {
    let _guard = serial();
    let out = ok(&cargo_make(&project(), &["check"]), "cargo make check");
    let cmds = commands(&out);
    assert_eq!(
        cmds.len(),
        3,
        "cargo make check должна выполнить ровно три команды (fmt, clippy, test), а выполнила {}: {cmds:?}",
        cmds.len()
    );
    assert!(cmds[0].contains("fmt"), "первой должна идти проверка форматирования: {cmds:?}");
    assert!(cmds[1].contains("clippy"), "второй должна идти проверка clippy: {cmds:?}");
    assert!(cmds[2].contains("test"), "третьей должна идти команда cargo test: {cmds:?}");
}

#[test]
fn check_does_not_run_the_builtin_cargo_check() {
    let _guard = serial();
    let out = ok(&cargo_make(&project(), &["check"]), "cargo make check");
    assert!(
        !commands(&out).iter().any(|c| c.trim() == "cargo check"),
        "в check затесалась встроенная команда cargo check: у cargo-make уже есть задача check, и свою нужно объявить так, чтобы встроенная не подмешивалась"
    );
}

#[test]
fn run_reads_the_env_file() {
    let _guard = serial();
    let o = cargo_make(&project(), &["run"]);
    ok(&o, "cargo make run");
    assert_eq!(stdout(&o), "ledger info", "run должна подхватить APP_NAME и LOG_LEVEL из .env");
}

#[test]
fn shell_variable_beats_the_env_file() {
    let _guard = serial();
    let o = run_env(&project(), "cargo", &["make", "run"], &[("LOG_LEVEL", "debug")]);
    ok(&o, "LOG_LEVEL=debug cargo make run");
    assert_eq!(
        stdout(&o),
        "ledger debug",
        "переменная из оболочки должна быть сильнее .env: так разработчик временно меняет настройку без правки файла"
    );
}

#[test]
fn env_example_lists_the_same_keys() {
    let _guard = serial();
    let keys = |name: &str| -> Vec<String> {
        let mut k: Vec<String> = fs::read_to_string(project().join(name))
            .expect("нет файла")
            .lines()
            .filter_map(|l| l.split_once('=').map(|(k, _)| k.trim().to_string()))
            .collect();
        k.sort();
        k
    };
    assert_eq!(keys(".env.example"), keys(".env"), ".env.example должен перечислять те же переменные, что и .env");
}

#[test]
fn fmt_and_lint_tasks_are_real_gates() {
    let _guard = serial();
    let dir = sandbox("cm_fmt");
    spoil_formatting(&dir, "src/main.rs");
    assert!(!cargo_make(&dir, &["fmt"]).status.success(), "задача fmt должна падать на плохом форматировании");
    let dir = sandbox("cm_lint");
    spoil_clippy(&dir, "src/main.rs");
    assert!(!cargo_make(&dir, &["lint"]).status.success(), "задача lint должна падать на предупреждениях clippy");
    let dir = sandbox("cm_check");
    spoil_clippy(&dir, "src/main.rs");
    assert!(!cargo_make(&dir, &["check"]).status.success(), "check должна падать, если падает lint");
}
