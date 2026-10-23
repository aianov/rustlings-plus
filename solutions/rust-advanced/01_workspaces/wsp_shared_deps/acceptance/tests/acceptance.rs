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

const MEMBERS: [&str; 3] = ["model", "store", "dump"];

#[test]
fn package_fields_are_inherited() {
    let _guard = serial();
    let meta = metadata(&project());
    for name in MEMBERS {
        let pkg = package(&meta, name);
        assert_eq!(pkg["version"], "0.5.0", "{name}: версия должна приходить из [workspace.package] (0.5.0)");
        assert_eq!(pkg["edition"], "2024", "{name}: edition должна приходить из [workspace.package] (2024)");
        assert_eq!(pkg["license"], "MIT", "{name}: license должна приходить из [workspace.package] (MIT)");
        assert_eq!(pkg["rust_version"], "1.88", "{name}: rust-version должна приходить из [workspace.package] (1.88)");
    }
}

#[test]
fn every_dependency_is_inherited_from_the_root() {
    let _guard = serial();
    let root = read_toml(&project().join("Cargo.toml"));
    let shared = root["workspace"]["dependencies"]
        .as_table()
        .expect("в корне нет [workspace.dependencies]");
    for key in ["serde", "serde_json", "model", "store"] {
        assert!(shared.contains_key(key), "в [workspace.dependencies] не хватает {key}");
    }
    let manifests = [
        project().join("crates/model/Cargo.toml"),
        project().join("crates/store/Cargo.toml"),
        project().join("tools/dump/Cargo.toml"),
    ];
    for path in manifests {
        let manifest = read_toml(&path);
        let deps = manifest["dependencies"].as_table().expect("нет [dependencies]");
        for (name, spec) in deps {
            let inherited = spec.get("workspace").and_then(|v| v.as_bool()) == Some(true);
            assert!(
                inherited,
                "{}: зависимость {name} должна быть записана через workspace = true, версию и путь знает только корень",
                path.display()
            );
        }
    }
}

#[test]
fn serde_resolves_to_a_single_version() {
    let _guard = serial();
    must_succeed(&cargo(&project(), &["build", "--workspace"]), "cargo build --workspace");
    let lock = fs::read_to_string(project().join("Cargo.lock")).expect("после сборки должен появиться Cargo.lock");
    let count = lock.matches("name = \"serde\"").count();
    assert_eq!(count, 1, "в Cargo.lock должна быть одна версия serde, а записей {count}");
}

#[test]
fn lints_come_from_the_workspace() {
    let _guard = serial();
    for (name, file) in [
        ("model", "crates/model/src/lib.rs"),
        ("store", "crates/store/src/lib.rs"),
        ("dump", "tools/dump/src/main.rs"),
    ] {
        let dir = sandbox(&format!("shared_lints_{name}"));
        let path = dir.join(file);
        let mut text = fs::read_to_string(&path).expect("нет исходника");
        text.push_str("
pub fn sneaky() -> u8 {
    unsafe { 1 }
}
");
        fs::write(&path, text).expect("не удалось записать исходник");
        let o = cargo(&dir, &["check", "-p", name]);
        let text = out(&o);
        assert!(
            !o.status.success() && text.contains("unsafe"),
            "{name}: unsafe в крейте должен ломать сборку. В корне нужен [workspace.lints], в {name} нужен [lints] workspace = true.
{text}"
        );
    }
}

#[test]
fn dump_keeps_insertion_order() {
    let _guard = serial();
    let o = cargo(&project(), &["run", "--quiet", "-p", "dump"]);
    let text = must_succeed(&o, "cargo run -p dump");
    assert!(
        text.contains("{\"zone\":\"eu\",\"id\":7,\"name\":\"nightly\"}"),
        "dump должен печатать ключи в порядке вставки (zone, id, name): добавь feature preserve_order только этому крейту, не трогая общую запись. Вывод:\n{text}"
    );
}

#[test]
fn shared_entry_stays_free_of_preserve_order() {
    let _guard = serial();
    let root = read_toml(&project().join("Cargo.toml"));
    let spec = &root["workspace"]["dependencies"]["serde_json"];
    let features = spec.get("features").and_then(|f| f.as_array()).map(|f| f.len()).unwrap_or(0);
    assert_eq!(features, 0, "feature preserve_order нужна только dump, в общую запись serde_json её класть не нужно");
}
