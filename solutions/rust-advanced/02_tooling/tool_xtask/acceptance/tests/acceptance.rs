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

fn xtask(dir: &Path, args: &[&str]) -> Output {
    let mut all = vec!["xtask"];
    all.extend_from_slice(args);
    cargo(dir, &all)
}

fn service_dirs(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir.join("services"))
        .expect("нет папки services")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    names.sort();
    names
}

#[test]
fn alias_exists_and_usage_is_printed() {
    let _guard = serial();
    let o = xtask(&project(), &[]);
    assert!(!o.status.success(), "cargo xtask без аргументов должен завершаться с ошибкой");
    let t = text(&o);
    assert!(
        t.contains("check") && t.contains("new-service"),
        "справка должна перечислять подкоманды check и new-service. Вывод:\n{t}"
    );
    let o = xtask(&project(), &["no-such-command"]);
    assert!(!o.status.success(), "неизвестная подкоманда это ошибка");
}

#[test]
fn new_service_creates_a_runnable_crate() {
    let _guard = serial();
    let dir = sandbox("xtask_new");
    let o = xtask(&dir, &["new-service", "billing"]);
    let t = ok(&o, "cargo xtask new-service billing");
    assert!(t.contains("services/billing"), "команда должна сообщить, что создала services/billing. Вывод:\n{t}");
    let manifest = read_toml(&dir.join("services/billing/Cargo.toml"));
    assert_eq!(manifest["package"]["name"].as_str(), Some("billing"), "имя пакета в манифесте");
    let run = cargo(&dir, &["run", "--quiet", "-p", "billing"]);
    assert_eq!(
        String::from_utf8_lossy(&run.stdout).trim(),
        "billing started",
        "новый сервис должен собираться (его подхватывает маска в members) и печатать `billing started`. Вывод:\n{}",
        text(&run)
    );
}

#[test]
fn bad_names_are_rejected_and_nothing_is_created() {
    let _guard = serial();
    let dir = sandbox("xtask_bad");
    let before = service_dirs(&dir);
    for bad in ["Billing", "9lives", "with space", "a/b", "../escape", "-dash", "UPPER", "snake_case"] {
        let o = xtask(&dir, &["new-service", bad]);
        assert!(!o.status.success(), "имя `{bad}` должно отвергаться");
        assert_eq!(service_dirs(&dir), before, "после отказа на `{bad}` ничего не должно появиться");
    }
    assert!(!dir.join("escape").exists() && !dir.join("services/../escape").exists());
}

#[test]
fn existing_service_is_not_overwritten() {
    let _guard = serial();
    let dir = sandbox("xtask_twice");
    ok(&xtask(&dir, &["new-service", "billing"]), "первое создание");
    let main_rs = dir.join("services/billing/src/main.rs");
    fs::write(&main_rs, "fn main() { println!(\"mine\"); }\n").expect("запись");
    let o = xtask(&dir, &["new-service", "billing"]);
    assert!(!o.status.success(), "повторное создание должно завершаться ошибкой");
    assert_eq!(
        fs::read_to_string(&main_rs).expect("чтение"),
        "fn main() { println!(\"mine\"); }\n",
        "правки ученика не должны затираться"
    );
}

#[test]
fn check_passes_on_clean_code_and_catches_problems() {
    let _guard = serial();
    let dir = sandbox("xtask_check_ok");
    ok(&xtask(&dir, &["check"]), "cargo xtask check");
    let dir = sandbox("xtask_check_fmt");
    spoil_formatting(&dir, "crates/core/src/lib.rs");
    assert!(!xtask(&dir, &["check"]).status.success(), "check должна падать на плохом форматировании");
    let dir = sandbox("xtask_check_clippy");
    spoil_clippy(&dir, "crates/core/src/lib.rs");
    assert!(!xtask(&dir, &["check"]).status.success(), "check должна падать на предупреждениях clippy");
}
