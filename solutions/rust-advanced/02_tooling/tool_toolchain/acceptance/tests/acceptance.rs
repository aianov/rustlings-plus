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

fn pinned(channel: &str) -> bool {
    let parts: Vec<&str> = channel.split('.').collect();
    parts.len() == 3 && parts.iter().all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
}

#[test]
fn toolchain_is_pinned_with_components() {
    let _guard = serial();
    let t = read_toml(&project().join("rust-toolchain.toml"));
    let tc = t["toolchain"].as_table().expect("нужна таблица [toolchain]");
    let channel = tc["channel"].as_str().expect("нужен channel");
    assert!(pinned(channel), "channel должен быть закреплённой версией вида 1.88.0, а не `{channel}`");
    let comps: Vec<&str> = tc["components"].as_array().expect("нужен components").iter().filter_map(|c| c.as_str()).collect();
    assert!(comps.contains(&"rustfmt") && comps.contains(&"clippy"), "в components нужны rustfmt и clippy: {comps:?}");
}

#[test]
fn formatting_width_comes_from_rustfmt_toml() {
    let _guard = serial();
    ok(&cargo(&project(), &["fmt", "--all", "--check"]), "cargo fmt --all --check");
    let dir = sandbox("tc_width");
    let path = dir.join("crates/shapes/src/lib.rs");
    let mut t = fs::read_to_string(&path).expect("нет файла");
    t.push_str("\npub fn weighted_area_with_a_long_name(width: f64, height: f64, weight: f64) -> f64 {\n    width * height * weight\n}\n");
    fs::write(&path, t).expect("запись");
    assert!(
        !cargo(&dir, &["fmt", "--all", "--check"]).status.success(),
        "строка в 85 символов должна считаться слишком длинной: нужен rustfmt.toml с max_width = 80"
    );
}

#[test]
fn clippy_rules_come_from_clippy_toml() {
    let _guard = serial();
    ok(&cargo(&project(), &["clippy", "--workspace", "--all-targets", "--", "-D", "warnings"]), "cargo clippy");
    let dir = sandbox("tc_exit");
    let path = dir.join("crates/shapes/src/lib.rs");
    let mut t = fs::read_to_string(&path).expect("нет файла");
    t.push_str("\npub fn bail() {\n    std::process::exit(1)\n}\n");
    fs::write(&path, t).expect("запись");
    let o = cargo(&dir, &["clippy", "-p", "shapes", "--", "-D", "warnings"]);
    assert!(
        !o.status.success() && text(&o).contains("disallowed"),
        "std::process::exit должен быть запрещён через disallowed-methods в clippy.toml. Вывод:\n{}",
        text(&o)
    );
    let dir = sandbox("tc_args");
    let path = dir.join("crates/shapes/src/lib.rs");
    let mut t = fs::read_to_string(&path).expect("нет файла");
    t.push_str("\npub fn many(a: u8, b: u8, c: u8, d: u8, e: u8) -> u8 {\n    a + b + c + d + e\n}\n");
    fs::write(&path, t).expect("запись");
    let o = cargo(&dir, &["clippy", "-p", "shapes", "--", "-D", "warnings"]);
    assert!(
        !o.status.success() && text(&o).contains("too_many_arguments"),
        "пять аргументов должны считаться слишком многими: too-many-arguments-threshold = 4. Вывод:\n{}",
        text(&o)
    );
}

#[test]
fn make_check_uses_the_lock_file() {
    let _guard = serial();
    let out = ok(&make(&project(), &["-n", "check"]), "make -n check");
    assert!(out.matches("--locked").count() >= 2, "clippy и test в check должны идти с --locked. Вывод:\n{out}");
    let dir = sandbox("tc_locked");
    ok(&cargo(&dir, &["generate-lockfile"]), "cargo generate-lockfile");
    ok(&make(&dir, &["check"]), "make check при наличии Cargo.lock");
    fs::remove_file(dir.join("Cargo.lock")).expect("удаление лока");
    assert!(!make(&dir, &["check"]).status.success(), "без Cargo.lock проверка с --locked должна падать");
}
