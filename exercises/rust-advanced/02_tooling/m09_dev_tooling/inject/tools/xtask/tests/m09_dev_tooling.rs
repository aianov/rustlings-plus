use std::fs;
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::{Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

static LOCK: Mutex<()> = Mutex::new(());

const FMT: &str = "fmt --all --check";
const LINT: &str = "clippy --workspace --all-targets -- -D warnings";
const TEST: &str = "test --workspace";

const FAKE_CARGO: &str = r#"#!/bin/sh
case "$1" in
  fmt|clippy|test|build|run|check)
    echo "$(pwd -P)|$*" >> "$FAKE_CARGO_LOG"
    if [ "$1" = "$FAKE_CARGO_FAIL" ]; then exit 7; fi
    exit 0 ;;
  *) exec "$REAL_CARGO" "$@" ;;
esac
"#;

fn serial() -> MutexGuard<'static, ()> {
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("tools/xtask лежит на два уровня ниже корня воркспейса")
        .to_path_buf()
}

fn sandbox(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("m09")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("временная папка теста создаётся");
    dir.canonicalize()
        .expect("путь временной папки разрешается")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn exec(mut cmd: Command) -> Output {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    for (key, _) in std::env::vars() {
        if key.starts_with("CARGO_MAKE") {
            cmd.env_remove(&key);
        }
    }
    cmd.env("CARGO_MAKE_DISABLE_UPDATE_CHECK", "1");
    let mut child = cmd.spawn().expect("команда запускается");
    let mut stdout = child.stdout.take().expect("у процесса есть stdout");
    let mut stderr = child.stderr.take().expect("у процесса есть stderr");
    let out = thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stdout.read_to_end(&mut bytes);
        bytes
    });
    let err = thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stderr.read_to_end(&mut bytes);
        bytes
    });
    let deadline = Instant::now() + Duration::from_secs(120);
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child.try_wait().expect("состояние процесса читается")
        {
            break status;
        }
        if Instant::now() > deadline {
            timed_out = true;
            let group = format!("-{}", child.id());
            let _ = Command::new("kill").args(["-KILL", &group]).status();
            let _ = child.kill();
            break child.wait().expect("процесс завершается после kill");
        }
        thread::sleep(Duration::from_millis(20));
    };
    let mut stderr = err.join().unwrap_or_default();
    if timed_out {
        stderr.extend_from_slice("\n[тест остановил команду: дольше 120 секунд]\n".as_bytes());
    }
    Output {
        status,
        stdout: out.join().unwrap_or_default(),
        stderr,
    }
}

fn real_cargo() -> String {
    std::env::var("CARGO").expect("cargo test задаёт переменную CARGO")
}

struct FakeCargo {
    script: PathBuf,
    log: PathBuf,
}

impl FakeCargo {
    fn new(dir: &Path) -> FakeCargo {
        let bin = dir.join("fakebin");
        fs::create_dir_all(&bin).expect("папка поддельного cargo создаётся");
        let script = bin.join("cargo");
        fs::write(&script, FAKE_CARGO).expect("поддельный cargo пишется");
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755))
            .expect("поддельный cargo становится исполняемым");
        FakeCargo {
            script,
            log: dir.join("cargo.log"),
        }
    }

    fn command(&self, program: &str, dir: &Path) -> Command {
        let bin = self.script.parent().expect("у скрипта есть папка");
        let path = std::env::var("PATH").unwrap_or_default();
        let mut cmd = Command::new(program);
        cmd.current_dir(dir)
            .env("PATH", format!("{}:{path}", bin.display()))
            .env("CARGO", &self.script)
            .env("FAKE_CARGO_LOG", &self.log)
            .env("REAL_CARGO", real_cargo())
            .env_remove("FAKE_CARGO_FAIL")
            .env_remove("SERVICE");
        cmd
    }

    fn calls(&self) -> Vec<(String, String)> {
        fs::read_to_string(&self.log)
            .unwrap_or_default()
            .lines()
            .filter_map(|line| line.split_once('|'))
            .map(|(dir, args)| (dir.to_string(), args.to_string()))
            .collect()
    }

    fn args(&self) -> Vec<String> {
        self.calls().into_iter().map(|(_, args)| args).collect()
    }
}

fn env_keys(path: &Path) -> Vec<String> {
    fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| line.split_once('='))
        .map(|(key, _)| key.trim().to_string())
        .collect()
}

fn env_value(path: &Path, wanted: &str) -> Option<String> {
    fs::read_to_string(path).ok()?.lines().find_map(|line| {
        let (key, value) = line.split_once('=')?;
        (key.trim() == wanted).then(|| value.trim().to_string())
    })
}

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("папка копии создаётся");
    for entry in fs::read_dir(from).expect("папка проекта читается") {
        let entry = entry.expect("запись папки читается");
        let name = entry.file_name();
        if name == "target" || name == ".git" {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            copy_tree(&path, &to.join(&name));
        } else {
            fs::copy(&path, to.join(&name)).expect("файл копируется");
        }
    }
}

fn probe_package(dir: &Path, name: &str, main: &str, config: &str) {
    fs::create_dir_all(dir.join("src")).expect("папка пробного пакета создаётся");
    let manifest = format!(
        "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[workspace]\n"
    );
    fs::write(dir.join("Cargo.toml"), manifest).expect("манифест пробного пакета пишется");
    fs::write(dir.join("src").join("main.rs"), main).expect("main.rs пробного пакета пишется");
    for file in [config, "rust-toolchain.toml"] {
        fs::copy(root().join(file), dir.join(file))
            .unwrap_or_else(|e| panic!("{file} в корне воркспейса должен быть: {e}"));
    }
}

#[test]
fn make_check_runs_fmt_lint_test_in_order() {
    let _guard = serial();
    let fake = FakeCargo::new(&sandbox("make-check"));
    let out = exec({
        let mut cmd = fake.command("make", &root());
        cmd.arg("check");
        cmd
    });
    assert!(
        out.status.success(),
        "make check с успешными шагами должен пройти:\n{}",
        text(&out.stderr)
    );
    assert_eq!(
        fake.args(),
        vec![FMT, LINT, TEST],
        "make check запускает ровно три шага по порядку: fmt, lint, test"
    );
}

#[test]
fn make_check_stops_on_first_failure() {
    let _guard = serial();
    let fake = FakeCargo::new(&sandbox("make-fail"));
    let out = exec({
        let mut cmd = fake.command("make", &root());
        cmd.arg("check").env("FAKE_CARGO_FAIL", "clippy");
        cmd
    });
    assert!(
        !out.status.success(),
        "если clippy упал, make check тоже падает"
    );
    assert_eq!(
        fake.args(),
        vec![FMT, LINT],
        "после упавшего lint тесты не запускаются"
    );
}

#[test]
fn make_run_picks_the_service() {
    let _guard = serial();
    let fake = FakeCargo::new(&sandbox("make-run"));
    let run = |args: &[&str], env: Option<&str>| {
        let mut cmd = fake.command("make", &root());
        cmd.arg("run").args(args);
        if let Some(service) = env {
            cmd.env("SERVICE", service);
        }
        let out = exec(cmd);
        assert!(
            out.status.success(),
            "make run должен пройти:\n{}",
            text(&out.stderr)
        );
    };
    run(&[], None);
    run(&["SERVICE=chat-service"], None);
    run(&[], Some("feed-service"));
    run(&["SERVICE=chat-service"], Some("feed-service"));
    assert_eq!(
        fake.args(),
        vec![
            "run --quiet -p repl",
            "run --quiet -p chat-service",
            "run --quiet -p feed-service",
            "run --quiet -p chat-service",
        ],
        "без SERVICE запускается repl; SERVICE берётся из окружения (?=), а аргумент make сильнее окружения"
    );
}

#[test]
fn make_targets_are_phony() {
    let _guard = serial();
    let dir = sandbox("make-phony");
    fs::copy(root().join("Makefile"), dir.join("Makefile")).expect("Makefile копируется");
    for target in ["build", "run", "test", "fmt", "lint", "check"] {
        fs::write(dir.join(target), "").expect("файл-приманка пишется");
    }
    let fake = FakeCargo::new(&dir);
    for target in ["test", "fmt"] {
        let out = exec({
            let mut cmd = fake.command("make", &dir);
            cmd.arg(target);
            cmd
        });
        assert!(
            out.status.success(),
            "make {target} должен пройти:\n{}",
            text(&out.stderr)
        );
    }
    assert_eq!(
        fake.args(),
        vec![TEST, FMT],
        "файлы с именами целей не должны мешать: все цели в .PHONY"
    );
}

#[test]
fn cargo_make_check_has_exactly_three_steps() {
    let _guard = serial();
    let fake = FakeCargo::new(&sandbox("cargo-make-check"));
    let out = exec({
        let mut cmd = fake.command("cargo-make", &root());
        cmd.args(["make", "check"]);
        cmd
    });
    assert!(
        out.status.success(),
        "cargo make check с успешными шагами должен пройти:\n{}",
        text(&out.stderr)
    );
    assert_eq!(
        fake.args(),
        vec![FMT, LINT, TEST],
        "cargo make check: fmt, clippy, тесты и больше ничего (встроенный cargo check убирает clear = true, обход членов воркспейса убирает default_to_workspace = false)"
    );
}

#[test]
fn cargo_make_check_stops_on_first_failure() {
    let _guard = serial();
    let fake = FakeCargo::new(&sandbox("cargo-make-fail"));
    let out = exec({
        let mut cmd = fake.command("cargo-make", &root());
        cmd.args(["make", "check"]).env("FAKE_CARGO_FAIL", "fmt");
        cmd
    });
    assert!(
        !out.status.success(),
        "если fmt упал, cargo make check тоже падает"
    );
    assert_eq!(
        fake.args(),
        vec![FMT],
        "после упавшего fmt остальные шаги не запускаются"
    );
}

#[test]
fn cargo_make_run_takes_service_from_env_file_but_shell_wins() {
    let _guard = serial();
    let from_file = env_value(&root().join(".env"), "SERVICE")
        .expect("в .env должен быть ключ SERVICE (скопируй .env.example)");
    let fake = FakeCargo::new(&sandbox("cargo-make-run"));
    for shell in [None, Some("chat-service")] {
        let mut cmd = fake.command("cargo-make", &root());
        cmd.args(["make", "run"]);
        if let Some(service) = shell {
            cmd.env("SERVICE", service);
        }
        let out = exec(cmd);
        assert!(
            out.status.success(),
            "cargo make run должен пройти:\n{}",
            text(&out.stderr)
        );
    }
    assert_eq!(
        fake.args(),
        vec![
            format!("run --quiet -p {from_file}"),
            String::from("run --quiet -p chat-service"),
        ],
        "SERVICE приходит из .env, но переменная оболочки сильнее файла (defaults_only = true)"
    );
}

#[test]
fn env_example_lists_every_variable() {
    let example = env_keys(&root().join(".env.example"));
    for key in ["SERVICE", "RUST_BACKTRACE"] {
        assert!(
            example.iter().any(|k| k == key),
            ".env.example должен содержать {key}"
        );
    }
    let local = env_keys(&root().join(".env"));
    for key in &example {
        assert!(
            local.contains(key),
            "в .env нет ключа {key} из .env.example"
        );
    }
}

#[test]
fn toolchain_is_pinned_exactly() {
    let toolchain = fs::read_to_string(root().join("rust-toolchain.toml"))
        .expect("rust-toolchain.toml в корне читается");
    let channel = toolchain
        .lines()
        .find_map(|line| line.trim().strip_prefix("channel = "))
        .map(|value| value.trim_matches('"').to_string())
        .expect("в rust-toolchain.toml есть channel");
    let parts: Vec<&str> = channel.split('.').collect();
    assert!(
        parts.len() == 3 && parts.iter().all(|p| p.parse::<u32>().is_ok()),
        "channel должен быть точной версией вида 1.88.0, а не {channel:?}"
    );
    for component in ["rustfmt", "clippy"] {
        assert!(
            toolchain.contains(&format!("\"{component}\"")),
            "в components rust-toolchain.toml нужен {component}"
        );
    }
}

#[test]
fn xtask_without_command_prints_usage() {
    let _guard = serial();
    for args in [&[][..], &["deploy"][..]] {
        let out = exec({
            let mut cmd = Command::new(env!("CARGO_BIN_EXE_xtask"));
            cmd.args(args).current_dir(root());
            cmd
        });
        let help = text(&out.stderr);
        assert!(
            !out.status.success(),
            "xtask {args:?} должен завершаться с ошибкой"
        );
        assert!(
            help.contains("check") && help.contains("new-service"),
            "справка xtask должна называть check и new-service, а напечатано:\n{help}"
        );
    }
}

#[test]
fn xtask_check_runs_three_steps_from_workspace_root() {
    let _guard = serial();
    let dir = sandbox("xtask-check");
    fs::write(dir.join("Cargo.toml"), "[workspace]\nmembers = []\n").expect("корень пишется");
    let deep = dir.join("src").join("crates").join("deep");
    fs::create_dir_all(&deep).expect("вложенная папка создаётся");
    let fake = FakeCargo::new(&dir);
    let mut cmd = fake.command(env!("CARGO_BIN_EXE_xtask"), &deep);
    cmd.arg("check");
    let out = exec(cmd);
    assert!(
        out.status.success(),
        "xtask check с успешными шагами должен пройти:\n{}",
        text(&out.stderr)
    );
    let root = dir.display().to_string();
    let expected: Vec<(String, String)> = [FMT, LINT, TEST]
        .iter()
        .map(|args| (root.clone(), args.to_string()))
        .collect();
    assert_eq!(
        fake.calls(),
        expected,
        "xtask check находит корень воркспейса (ближайший Cargo.toml с [workspace] выше текущей папки), запускает три шага оттуда и тем cargo, что лежит в переменной CARGO"
    );
}

#[test]
fn xtask_check_stops_on_first_failure() {
    let _guard = serial();
    let dir = sandbox("xtask-fail");
    fs::write(dir.join("Cargo.toml"), "[workspace]\nmembers = []\n").expect("корень пишется");
    let fake = FakeCargo::new(&dir);
    let mut cmd = fake.command(env!("CARGO_BIN_EXE_xtask"), &dir);
    cmd.arg("check").env("FAKE_CARGO_FAIL", "clippy");
    let out = exec(cmd);
    assert!(
        !out.status.success(),
        "если clippy упал, xtask check тоже падает"
    );
    assert!(
        text(&out.stderr).contains("clippy"),
        "сообщение об ошибке называет упавший шаг, а напечатано:\n{}",
        text(&out.stderr)
    );
    assert_eq!(
        fake.args(),
        vec![FMT, LINT],
        "после упавшего шага остальные не запускаются"
    );
}

fn new_service(dir: &Path, name: &str) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_xtask"));
    cmd.args(["new-service", name]).current_dir(dir);
    exec(cmd)
}

#[test]
fn xtask_new_service_creates_template_and_registers_mask() {
    let _guard = serial();
    let dir = sandbox("xtask-new");
    fs::copy(root().join("Cargo.toml"), dir.join("Cargo.toml")).expect("корень копируется");
    let out = new_service(&dir, "chat-service");
    assert!(
        out.status.success(),
        "new-service chat-service должен пройти:\n{}",
        text(&out.stderr)
    );
    let service = dir.join("src").join("microservices").join("chat-service");
    let manifest = fs::read_to_string(service.join("Cargo.toml"))
        .expect("в src/microservices/chat-service есть Cargo.toml");
    for line in [
        "name = \"chat-service\"",
        "version.workspace = true",
        "edition.workspace = true",
        "[lints]",
        "workspace = true",
    ] {
        assert!(
            manifest.lines().any(|l| l.trim() == line),
            "в Cargo.toml нового сервиса нужна строка `{line}`, а там:\n{manifest}"
        );
    }
    let main = fs::read_to_string(service.join("src").join("main.rs"))
        .expect("в src/microservices/chat-service есть src/main.rs");
    assert!(
        main.contains("println!(\"chat-service started\")"),
        "main.rs нового сервиса печатает `chat-service started`, а там:\n{main}"
    );
    let root_manifest = fs::read_to_string(dir.join("Cargo.toml")).expect("корень читается");
    assert!(
        root_manifest.contains("\"src/microservices/*\""),
        "new-service добавляет маску \"src/microservices/*\" в members, если её ещё нет"
    );
    let again = new_service(&dir, "chat-service");
    assert!(!again.status.success(), "второй chat-service не создаётся");
    assert_eq!(
        fs::read_to_string(service.join("src").join("main.rs")).unwrap_or_default(),
        main,
        "существующий сервис не перезаписывается"
    );
    let second = new_service(&dir, "feed-service");
    assert!(
        second.status.success(),
        "второй сервис создаётся:\n{}",
        text(&second.stderr)
    );
    let root_manifest = fs::read_to_string(dir.join("Cargo.toml")).expect("корень читается");
    assert_eq!(
        root_manifest.matches("\"src/microservices/*\"").count(),
        1,
        "маска в members появляется один раз"
    );
}

#[test]
fn xtask_new_service_rejects_bad_names() {
    let _guard = serial();
    let dir = sandbox("xtask-bad");
    fs::copy(root().join("Cargo.toml"), dir.join("Cargo.toml")).expect("корень копируется");
    let before = fs::read_to_string(dir.join("Cargo.toml")).expect("корень читается");
    for name in [
        "",
        "Chat",
        "1chat",
        "chat_service",
        "chat--service",
        "chat-",
        "../evil",
        "a/b",
    ] {
        let out = new_service(&dir, name);
        assert!(!out.status.success(), "имя {name:?} должно отвергаться");
    }
    assert!(
        !dir.join("src").exists() && !dir.join("evil").exists(),
        "после плохих имён на диске ничего не появляется"
    );
    assert_eq!(
        fs::read_to_string(dir.join("Cargo.toml")).unwrap_or_default(),
        before,
        "после плохих имён Cargo.toml корня не меняется"
    );
}

#[test]
fn new_service_builds_inside_the_real_workspace() {
    let _guard = serial();
    let dir = sandbox("workspace-copy");
    let copy = dir.join("backend");
    copy_tree(&root(), &copy);
    let target = dir.join("target");
    let cargo = |args: &[&str]| {
        let mut cmd = Command::new(real_cargo());
        cmd.args(args)
            .current_dir(&copy)
            .env("CARGO_TARGET_DIR", &target)
            .env_remove("CLIPPY_CONF_DIR");
        exec(cmd)
    };
    let out = cargo(&["xtask", "new-service", "chat-service"]);
    assert!(
        out.status.success(),
        "`cargo xtask new-service chat-service` должен работать через алиас из .cargo/config.toml:\n{}",
        text(&out.stderr)
    );
    let out = cargo(&["run", "--quiet", "-p", "chat-service"]);
    assert!(
        out.status.success(),
        "новый сервис должен собираться в воркспейсе без ручных правок:\n{}",
        text(&out.stderr)
    );
    assert_eq!(
        text(&out.stdout),
        "chat-service started\n",
        "новый сервис печатает `chat-service started`"
    );
}

#[test]
fn rustfmt_rules_apply() {
    let _guard = serial();
    let dir = sandbox("fmt-probe");
    let long = "struct Point {\n    x: i32,\n}\n\nfn main() {\n    let x = 1;\n    let point = Point { x: x };\n    println!(\"{}\", point.x);\n}\n";
    probe_package(&dir, "fmt-probe", long, "rustfmt.toml");
    let mut cmd = Command::new(real_cargo());
    cmd.args(["fmt", "--check"]).current_dir(&dir);
    let out = exec(cmd);
    assert!(
        !out.status.success() && text(&out.stdout).contains("Point { x }"),
        "rustfmt.toml с use_field_init_shorthand = true должен требовать `Point {{ x }}` вместо `Point {{ x: x }}`:\n{}{}",
        text(&out.stdout),
        text(&out.stderr)
    );
}

#[test]
fn clippy_rules_apply() {
    let _guard = serial();
    let dir = sandbox("clippy-probe");
    probe_package(
        &dir,
        "clippy-probe",
        "fn main() {\n    std::process::exit(0)\n}\n",
        "clippy.toml",
    );
    let mut cmd = Command::new(real_cargo());
    cmd.args(["clippy", "--quiet", "--", "-D", "warnings"])
        .current_dir(&dir)
        .env("CARGO_TARGET_DIR", dir.join("target"))
        .env_remove("CLIPPY_CONF_DIR");
    let out = exec(cmd);
    let report = text(&out.stderr);
    assert!(
        !out.status.success()
            && report.contains("disallowed")
            && report.contains("std::process::exit"),
        "clippy.toml должен запрещать std::process::exit (disallowed-methods):\n{report}"
    );
}
