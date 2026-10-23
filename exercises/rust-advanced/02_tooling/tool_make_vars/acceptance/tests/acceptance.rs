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
fn default_service_is_api() {
    let _guard = serial();
    let o = make(&project(), &["run"]);
    ok(&o, "make run");
    assert_eq!(stdout(&o), "api: ready", "без аргументов должен запускаться api");
}

#[test]
fn service_is_chosen_on_the_command_line() {
    let _guard = serial();
    let o = make(&project(), &["run", "SERVICE=auth"]);
    ok(&o, "make run SERVICE=auth");
    assert_eq!(stdout(&o), "auth: ready", "make run SERVICE=auth должен запускать auth");
}

#[test]
fn service_is_chosen_from_the_environment() {
    let _guard = serial();
    let o = run_env(&project(), "make", &["run"], &[("SERVICE", "auth")]);
    ok(&o, "SERVICE=auth make run");
    assert_eq!(
        stdout(&o),
        "auth: ready",
        "SERVICE=auth make run должен запускать auth: значение по умолчанию не должно перебивать окружение"
    );
}

#[test]
fn command_line_beats_environment() {
    let _guard = serial();
    let o = run_env(&project(), "make", &["run", "SERVICE=api"], &[("SERVICE", "auth")]);
    ok(&o, "make run SERVICE=api");
    assert_eq!(stdout(&o), "api: ready", "аргумент командной строки сильнее окружения");
}

#[test]
fn unknown_service_fails() {
    let _guard = serial();
    assert!(!make(&project(), &["run", "SERVICE=nope"]).status.success(), "несуществующий сервис должен давать ошибку");
}

#[test]
fn release_profile_adds_the_flag() {
    let _guard = serial();
    let release = ok(&make(&project(), &["-n", "run", "PROFILE=release"]), "make -n run PROFILE=release");
    assert!(release.contains("--release"), "PROFILE=release должен добавлять --release. Вывод:\n{release}");
    let debug = ok(&make(&project(), &["-n", "run"]), "make -n run");
    assert!(!debug.contains("--release"), "без PROFILE флага --release быть не должно. Вывод:\n{debug}");
    let other = ok(&make(&project(), &["-n", "run", "PROFILE=debug"]), "make -n run PROFILE=debug");
    assert!(!other.contains("--release"), "PROFILE=debug не release. Вывод:\n{other}");
}

#[test]
fn build_target_follows_the_same_profile() {
    let _guard = serial();
    let release = ok(&make(&project(), &["-n", "build", "PROFILE=release"]), "make -n build PROFILE=release");
    assert!(release.contains("cargo build") && release.contains("--release"), "make build тоже должна учитывать PROFILE:\n{release}");
}

#[test]
fn run_and_build_are_phony() {
    let _guard = serial();
    let dir = sandbox("vars_phony");
    fs::write(dir.join("run"), "").expect("не удалось создать файл");
    fs::create_dir_all(dir.join("build")).expect("не удалось создать папку");
    let out = ok(&make(&dir, &["-n", "run"]), "make -n run");
    assert!(out.contains("cargo run"), "make run не должен зависеть от файла run. Вывод:\n{out}");
    let out = ok(&make(&dir, &["-n", "build"]), "make -n build");
    assert!(out.contains("cargo build"), "make build не должен зависеть от папки build. Вывод:\n{out}");
}
