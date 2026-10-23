use std::fs;
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const FAKE_CARGO: &str = r#"#!/bin/sh
echo "$(pwd -P)|$*" >> "$FAKE_CARGO_LOG"
echo "медиана 1.0 мкс, p99 2.0 мкс, замеров 3"
exit "${FAKE_CARGO_EXIT:-0}"
"#;

fn sandbox(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("m16")
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
    let mut child = cmd.spawn().expect("xtask запускается");
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
    let deadline = Instant::now() + Duration::from_secs(60);
    let status = loop {
        if let Some(status) = child.try_wait().expect("состояние процесса читается")
        {
            break status;
        }
        if Instant::now() > deadline {
            let group = format!("-{}", child.id());
            let _ = Command::new("kill").args(["-KILL", &group]).status();
            let _ = child.kill();
            panic!("xtask работает дольше 60 секунд, похоже на зависание");
        }
        thread::sleep(Duration::from_millis(20));
    };
    Output {
        status,
        stdout: out.join().unwrap_or_default(),
        stderr: err.join().unwrap_or_default(),
    }
}

struct Bench {
    root: PathBuf,
    deep: PathBuf,
    script: PathBuf,
    log: PathBuf,
}

impl Bench {
    fn new(name: &str) -> Bench {
        let root = sandbox(name);
        fs::write(root.join("Cargo.toml"), "[workspace]\nmembers = []\n")
            .expect("манифест песочницы пишется");
        let deep = root.join("src").join("crates");
        fs::create_dir_all(&deep).expect("вложенная папка создаётся");
        let script = root.join("fakebin").join("cargo");
        fs::create_dir_all(root.join("fakebin")).expect("папка поддельного cargo создаётся");
        fs::write(&script, FAKE_CARGO).expect("поддельный cargo пишется");
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755))
            .expect("поддельный cargo становится исполняемым");
        let log = root.join("cargo.log");
        Bench {
            root,
            deep,
            script,
            log,
        }
    }

    fn run(&self, args: &[&str], exit: &str) -> Output {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_xtask"));
        cmd.args(args)
            .current_dir(&self.deep)
            .env("CARGO", &self.script)
            .env("FAKE_CARGO_LOG", &self.log)
            .env("FAKE_CARGO_EXIT", exit);
        exec(cmd)
    }

    fn calls(&self) -> Vec<(String, Vec<String>)> {
        fs::read_to_string(&self.log)
            .unwrap_or_default()
            .lines()
            .filter_map(|line| line.split_once('|'))
            .map(|(dir, args)| {
                let args = args.split_whitespace().map(String::from).collect();
                (dir.to_string(), args)
            })
            .collect()
    }
}

fn has_pair(args: &[String], keys: &[&str], value: &str) -> bool {
    args.windows(2)
        .any(|pair| keys.contains(&pair[0].as_str()) && pair[1] == value)
}

#[test]
fn xtask_bench_runs_the_feed_example_in_release() {
    let bench = Bench::new("bench-ok");
    let out = bench.run(&["bench"], "0");
    assert!(
        out.status.success(),
        "xtask bench с успешным замером должен пройти:\n{}",
        text(&out.stderr)
    );
    let calls = bench.calls();
    assert_eq!(
        calls.len(),
        1,
        "xtask bench зовёт cargo ровно один раз, а вызовы такие: {calls:?}"
    );
    let (dir, args) = &calls[0];
    assert_eq!(
        Path::new(dir),
        bench.root,
        "замер запускается из корня воркспейса (ближайший Cargo.toml с [workspace] выше текущей папки)"
    );
    assert_eq!(
        args.first().map(String::as_str),
        Some("run"),
        "замер это `cargo run ...`, а вызвано: cargo {}",
        args.join(" ")
    );
    assert!(
        args.iter().any(|arg| arg == "--release" || arg == "-r"),
        "замер без --release меряет отладочную сборку, такие числа ничего не значат: cargo {}",
        args.join(" ")
    );
    assert!(
        has_pair(args, &["-p", "--package"], "social-core"),
        "замер запускает пример пакета social-core (-p social-core): cargo {}",
        args.join(" ")
    );
    assert!(
        has_pair(args, &["--example"], "feed_bench"),
        "замер это пример feed_bench (--example feed_bench): cargo {}",
        args.join(" ")
    );
    assert!(
        text(&out.stdout).contains("медиана 1.0 мкс, p99 2.0 мкс"),
        "вывод замера доходит до терминала как есть, а напечатано:\n{}",
        text(&out.stdout)
    );
}

#[test]
fn xtask_bench_fails_when_the_bench_fails() {
    let bench = Bench::new("bench-fail");
    let out = bench.run(&["bench"], "3");
    assert!(
        !out.status.success(),
        "пример замера упал, значит и xtask bench падает с ненулевым кодом"
    );
}

#[test]
fn xtask_usage_names_bench() {
    let bench = Bench::new("bench-usage");
    let out = bench.run(&[], "0");
    assert!(
        !out.status.success(),
        "xtask без команды завершается с ошибкой"
    );
    let help = text(&out.stderr);
    assert!(
        help.contains("bench") && help.contains("check") && help.contains("new-service"),
        "справка xtask называет check, bench и new-service, а напечатано:\n{help}"
    );
    assert!(bench.calls().is_empty(), "без команды cargo не запускается");
}
