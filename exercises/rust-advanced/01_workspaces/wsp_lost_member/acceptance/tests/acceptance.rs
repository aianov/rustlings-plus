#![allow(dead_code)]
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn serial() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
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

fn project() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("папка acceptance лежит внутри проекта")
        .to_path_buf()
}

fn cargo_bin() -> String {
    std::env::var("CARGO").unwrap_or_else(|_| "cargo".into())
}

fn shared_target() -> PathBuf {
    Path::new(env!("CARGO_TARGET_TMPDIR")).join("shared")
}

fn cargo(dir: &Path, args: &[&str]) -> Output {
    let (sub, rest) = args.split_first().expect("нужна подкоманда cargo");
    let mut cmd = Command::new(cargo_bin());
    cmd.arg(sub).arg("--target-dir").arg(shared_target()).args(rest).env_remove("CLIPPY_CONF_DIR").current_dir(dir);
    exec(cmd, None)
}

fn cargo_local(dir: &Path, args: &[&str]) -> Output {
    let mut cmd = Command::new(cargo_bin());
    cmd.args(args).env_remove("CLIPPY_CONF_DIR").env_remove("CARGO_TARGET_DIR").current_dir(dir);
    exec(cmd, None)
}

fn out(o: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
}

fn must_succeed(o: &Output, what: &str) -> String {
    assert!(o.status.success(), "{what} завершилась с ошибкой:\n{}", out(o));
    out(o)
}

fn metadata(dir: &Path) -> serde_json::Value {
    let mut cmd = Command::new(cargo_bin());
    cmd.args(["metadata", "--format-version", "1", "--no-deps"]).current_dir(dir);
    let o = exec(cmd, None);
    assert!(
        o.status.success(),
        "cargo не смог прочитать воркспейс:\n{}",
        String::from_utf8_lossy(&o.stderr)
    );
    serde_json::from_slice(&o.stdout).expect("metadata не похожа на JSON")
}

fn package<'a>(meta: &'a serde_json::Value, name: &str) -> &'a serde_json::Value {
    meta["packages"]
        .as_array()
        .expect("в metadata нет packages")
        .iter()
        .find(|p| p["name"] == name)
        .unwrap_or_else(|| panic!("в воркспейсе нет пакета {name}"))
}

fn member_names(meta: &serde_json::Value) -> Vec<String> {
    let mut names: Vec<String> = meta["packages"]
        .as_array()
        .expect("в metadata нет packages")
        .iter()
        .map(|p| p["name"].as_str().unwrap_or_default().to_string())
        .collect();
    names.sort();
    names
}

fn dep_names(pkg: &serde_json::Value, kind: Option<&str>) -> Vec<String> {
    let mut names: Vec<String> = pkg["dependencies"]
        .as_array()
        .expect("нет dependencies")
        .iter()
        .filter(|d| d["kind"].as_str() == kind)
        .map(|d| d["name"].as_str().unwrap_or_default().to_string())
        .collect();
    names.sort();
    names
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

fn files_named(dir: &Path, name: &str, found: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("не удалось прочитать папку").flatten() {
        let path = entry.path();
        if path.is_dir() {
            files_named(&path, name, found);
        } else if entry.file_name() == name {
            found.push(path);
        }
    }
}

fn dirs_named(dir: &Path, name: &str, found: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("не удалось прочитать папку").flatten() {
        let path = entry.path();
        if path.is_dir() {
            if entry.file_name() == name {
                found.push(path.clone());
            }
            dirs_named(&path, name, found);
        }
    }
}

fn read_toml(path: &Path) -> toml::Table {
    let text = fs::read_to_string(path).unwrap_or_else(|_| panic!("нет файла {}", path.display()));
    text.parse::<toml::Table>().unwrap_or_else(|e| panic!("{} не разбирается как TOML: {e}", path.display()))
}

#[test]
fn all_four_crates_belong_to_the_workspace() {
    let _guard = serial();
    let meta = metadata(&project());
    assert_eq!(
        member_names(&meta),
        ["audit", "importer", "ledger-api", "ledger-core"],
        "в воркспейсе должны быть все четыре крейта: audit, importer, ledger-api, ledger-core"
    );
}

#[test]
fn workspace_test_runs_the_audit_tests() {
    let _guard = serial();
    let o = cargo(&project(), &["test", "--workspace"]);
    let text = must_succeed(&o, "cargo test --workspace");
    assert!(
        text.contains("audit_flags_negative_balance ... ok"),
        "cargo test --workspace не запустил тесты крейта audit: он должен быть членом воркспейса. Вывод:\n{text}"
    );
}

#[test]
fn working_inside_any_crate_keeps_one_lock_and_one_target() {
    let _guard = serial();
    let dir = sandbox("lost_single");
    must_succeed(&cargo_local(&dir, &["build", "--workspace"]), "cargo build --workspace");
    for inner in ["crates/audit", "crates/api", "tools/importer"] {
        let o = cargo_local(&dir.join(inner), &["build"]);
        must_succeed(&o, &format!("cargo build внутри {inner}"));
    }
    let mut locks = Vec::new();
    files_named(&dir, "Cargo.lock", &mut locks);
    assert_eq!(locks.len(), 1, "Cargo.lock должен быть один, в корне, а нашлось: {locks:?}");
    let mut targets = Vec::new();
    dirs_named(&dir, "target", &mut targets);
    assert_eq!(targets, [dir.join("target")], "target должен быть один, в корне, а нашлось: {targets:?}");
}

#[test]
fn importer_still_works() {
    let _guard = serial();
    let o = cargo(&project(), &["run", "--quiet", "-p", "importer", "--", "4", "5", "-1"]);
    let text = must_succeed(&o, "cargo run -p importer");
    assert!(text.contains("imported 3 rows, total 8"), "importer должен печатать `imported 3 rows, total 8`, а напечатал:\n{text}");
}

#[test]
fn new_crates_join_automatically() {
    let _guard = serial();
    let dir = sandbox("lost_new");
    let extra = dir.join("tools/fresh");
    fs::create_dir_all(extra.join("src")).expect("не удалось создать крейт");
    fs::write(extra.join("Cargo.toml"), "[package]\nname = \"fresh\"\nversion = \"0.1.0\"\nedition = \"2024\"\n")
        .expect("не удалось записать манифест");
    fs::write(extra.join("src/main.rs"), "fn main() {}\n").expect("не удалось записать main.rs");
    let names = member_names(&metadata(&dir));
    assert!(
        names.contains(&"fresh".to_string()),
        "новый крейт tools/fresh не попал в воркспейс: чтобы про членов не забывали, перечисли папки масками"
    );
}
