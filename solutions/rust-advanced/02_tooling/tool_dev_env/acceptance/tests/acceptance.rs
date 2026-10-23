#![allow(dead_code)]
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

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

fn keys(name: &str) -> Vec<String> {
    let mut k: Vec<String> = fs::read_to_string(project().join(name))
        .unwrap_or_else(|_| panic!("нет файла {name}"))
        .lines()
        .filter_map(|l| l.split_once('=').map(|(k, _)| k.trim().to_string()))
        .collect();
    k.sort();
    k
}

#[test]
fn all_the_files_are_in_place() {
    let _guard = serial();
    for rel in [
        "Makefile",
        "Makefile.toml",
        ".env.example",
        ".cargo/config.toml",
        "rust-toolchain.toml",
        "rustfmt.toml",
        "clippy.toml",
        "tools/xtask/Cargo.toml",
        "tools/xtask/src/main.rs",
    ] {
        assert!(project().join(rel).exists(), "не хватает файла {rel}");
    }
    assert_eq!(keys(".env.example"), keys(".env"), ".env.example должен перечислять те же переменные, что и .env");
}

#[test]
fn compiler_and_style_settings_are_set() {
    let _guard = serial();
    let tc = read_toml(&project().join("rust-toolchain.toml"));
    let channel = tc["toolchain"]["channel"].as_str().expect("нужен channel");
    let parts: Vec<&str> = channel.split('.').collect();
    assert!(
        parts.len() == 3 && parts.iter().all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit())),
        "версия компилятора должна быть закреплена точно (вида 1.88.0), а не `{channel}`"
    );
    let comps = tc["toolchain"]["components"].as_array().expect("нужен components");
    assert!(comps.iter().any(|c| c.as_str() == Some("rustfmt")) && comps.iter().any(|c| c.as_str() == Some("clippy")));
    let fmt = read_toml(&project().join("rustfmt.toml"));
    assert!(fmt.contains_key("max_width"), "rustfmt.toml должен задавать max_width");
    let clippy = read_toml(&project().join("clippy.toml"));
    assert!(clippy.contains_key("disallowed-methods"), "clippy.toml должен запрещать хотя бы один метод");
    ok(&cargo(&project(), &["fmt", "--all", "--check"]), "cargo fmt --all --check");
}

#[test]
fn make_run_takes_a_service_and_check_is_ordered() {
    let _guard = serial();
    let out = ok(&make(&project(), &["run", "SERVICE=auth"]), "make run SERVICE=auth");
    assert!(out.contains("auth started"), "make run SERVICE=auth должен запускать auth:\n{out}");
    let env_out = run_env(&project(), "make", &["run"], &[("SERVICE", "worker")]);
    assert!(text(&env_out).contains("worker started"), "SERVICE из окружения тоже работает:\n{}", text(&env_out));
    let plan = ok(&make(&project(), &["-n", "check"]), "make -n check");
    let (f, l, t) = (index_of(&plan, "cargo fmt"), index_of(&plan, "cargo clippy"), index_of(&plan, "cargo test"));
    assert!(f < l && l < t, "check идёт в порядке fmt, lint, test:\n{plan}");
}

#[test]
fn make_targets_do_not_depend_on_files_with_the_same_name() {
    let _guard = serial();
    let dir = sandbox("env_phony");
    for name in ["run", "check", "fmt", "lint", "test", "build"] {
        fs::write(dir.join(name), "").expect("создание файла");
    }
    for target in ["check", "fmt", "lint", "test", "build"] {
        let plan = ok(&make(&dir, &["-n", target]), &format!("make -n {target}"));
        assert!(plan.contains("cargo"), "make {target} ничего не делает из-за одноимённого файла:\n{plan}");
    }
    let plan = ok(&make(&dir, &["-n", "run"]), "make -n run");
    assert!(plan.contains("cargo run"), "make run ничего не делает из-за одноимённого файла:\n{plan}");
}

#[test]
fn cargo_make_check_has_three_steps() {
    let _guard = serial();
    let out = ok(&cargo_make(&project(), &["check"]), "cargo make check");
    let cmds = commands(&out);
    assert_eq!(cmds.len(), 3, "cargo make check: ровно fmt, clippy, test, а выполнено {cmds:?}");
    assert!(cmds[0].contains("fmt") && cmds[1].contains("clippy") && cmds[2].contains("test"), "порядок: {cmds:?}");
}

#[test]
fn run_all_is_parallel_and_reads_the_env_file() {
    let _guard = serial();
    ok(&cargo(&project(), &["build", "--workspace"]), "cargo build --workspace");
    let begin = Instant::now();
    let out = ok(&cargo_make(&project(), &["run-all"]), "cargo make run-all");
    let spent = begin.elapsed();
    for s in ["api", "auth", "worker"] {
        assert!(out.contains(&format!("{s} started")), "{s} не стартовал:\n{out}");
        assert!(
            out.contains(&format!("{s} stopped after 3000ms")),
            "{s} не получил SERVICE_SLEEP_MS=3000 из .env: подключи env_files.\n{out}"
        );
    }
    assert!(spent < Duration::from_millis(8000), "три сервиса по 3 секунды заняли {spent:?}: запуск не параллельный");
}

#[test]
fn shell_variable_beats_the_env_file_in_run_all() {
    let _guard = serial();
    ok(&cargo(&project(), &["build", "--workspace"]), "cargo build --workspace");
    let o = run_env(&project(), "cargo", &["make", "run-all"], &[("SERVICE_SLEEP_MS", "10")]);
    let out = ok(&o, "SERVICE_SLEEP_MS=10 cargo make run-all");
    assert!(out.contains("api stopped after 10ms"), "значение из оболочки должно побеждать .env:\n{out}");
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
