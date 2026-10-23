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

const SRC: &str = "crates/shopcore/src";

const EXPECTED_FILES: [&str; 11] = [
    "lib.rs",
    "billing/mod.rs",
    "billing/invoice.rs",
    "billing/money.rs",
    "billing/tax.rs",
    "catalog/mod.rs",
    "catalog/product.rs",
    "catalog/stock.rs",
    "customers/mod.rs",
    "customers/account.rs",
    "customers/loyalty.rs",
];

fn rs_files(dir: &Path, found: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("не удалось прочитать папку").flatten() {
        let path = entry.path();
        if path.is_dir() {
            rs_files(&path, found);
        } else if path.extension().is_some_and(|e| e == "rs") {
            found.push(path);
        }
    }
}

fn layout_problems(dir: &Path, problems: &mut Vec<String>) {
    let mut files = Vec::new();
    let mut dirs = Vec::new();
    for entry in fs::read_dir(dir).expect("не удалось прочитать папку").flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if entry.path().is_dir() {
            dirs.push(name);
        } else if name.ends_with(".rs") && !["mod.rs", "lib.rs", "main.rs"].contains(&name.as_str()) {
            files.push(name);
        }
    }
    for d in &dirs {
        if !dir.join(d).join("mod.rs").exists() {
            problems.push(format!("в папке модуля {} нет mod.rs", dir.join(d).display()));
        }
        if files.contains(&format!("{d}.rs")) {
            problems.push(format!("{d}.rs лежит рядом с папкой {d}/: выбери один вид раскладки"));
        }
        layout_problems(&dir.join(d), problems);
    }
}

#[test]
fn every_module_is_where_the_plan_says() {
    let _guard = serial();
    for rel in EXPECTED_FILES {
        let path = project().join(SRC).join(rel);
        assert!(path.exists(), "не вижу файла {SRC}/{rel}: разложи модули по плану из README");
    }
}

#[test]
fn layout_is_uniform() {
    let _guard = serial();
    let mut problems = Vec::new();
    layout_problems(&project().join(SRC), &mut problems);
    assert!(problems.is_empty(), "раскладка модулей неединая:\n{}", problems.join("\n"));
}

#[test]
fn files_stay_small() {
    let _guard = serial();
    let mut files = Vec::new();
    rs_files(&project().join(SRC), &mut files);
    for file in files {
        let lines = fs::read_to_string(&file).expect("не удалось прочитать файл").lines().count();
        assert!(lines <= 50, "{} длиннее 50 строк ({lines}): код должен быть разнесён по модулям", file.display());
    }
}

#[test]
fn lib_rs_is_only_a_table_of_contents() {
    let _guard = serial();
    let text = fs::read_to_string(project().join(SRC).join("lib.rs")).expect("нет lib.rs");
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with("//")) {
        let allowed = line.starts_with("mod ") || line.starts_with("pub mod ") || line.starts_with("pub use ") || line.starts_with("use ");
        assert!(allowed, "в lib.rs не должно быть кода, только mod и pub use, а нашлось: {line}");
    }
}

#[test]
fn public_api_works() {
    let _guard = serial();
    must_succeed(&cargo(&project(), &["test", "--workspace"]), "cargo test --workspace");
}

#[test]
fn inner_paths_are_not_public() {
    let _guard = serial();
    let dir = sandbox("layout_deep");
    fs::write(
        dir.join("crates/shopcore/tests/deep.rs"),
        "use shopcore::billing::invoice::Invoice;\n\n#[test]\nfn deep() {\n    let _ = Invoice::new();\n}\n",
    )
    .expect("не удалось записать пробный тест");
    let o = cargo(&dir, &["test", "-p", "shopcore", "--test", "deep"]);
    assert!(
        !o.status.success(),
        "путь shopcore::billing::invoice::Invoice всё ещё открыт снаружи: модули billing, catalog и customers должны быть приватными, а наружу выходит только фасад"
    );
}
