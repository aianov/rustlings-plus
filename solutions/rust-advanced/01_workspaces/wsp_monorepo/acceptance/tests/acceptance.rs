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


const ENGINES: [(&str, &str); 3] = [
    ("contracts", "src/crates/contracts"),
    ("text-engine", "src/crates/text-engine"),
    ("store-engine", "src/crates/store-engine"),
];
const SERVICES: [(&str, &str); 2] = [
    ("greeter-service", "src/microservices/greeter-service"),
    ("archive-service", "src/microservices/archive-service"),
];

fn all_members() -> Vec<(&'static str, &'static str)> {
    ENGINES.iter().chain(SERVICES.iter()).copied().collect()
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
            problems.push(format!("{d}.rs лежит рядом с папкой {d}/"));
        }
        layout_problems(&dir.join(d), problems);
    }
}

#[test]
fn workspace_has_five_members_found_by_masks() {
    let _guard = serial();
    let meta = metadata(&project());
    assert_eq!(
        member_names(&meta),
        ["archive-service", "contracts", "greeter-service", "store-engine", "text-engine"],
        "в воркспейсе должны быть пять крейтов: contracts, text-engine, store-engine, greeter-service, archive-service"
    );
    let root = read_toml(&project().join("Cargo.toml"));
    assert!(!root.contains_key("package"), "корень должен быть виртуальным: без [package]");
    let members: Vec<String> = root["workspace"]["members"]
        .as_array()
        .expect("нет workspace.members")
        .iter()
        .filter_map(|m| m.as_str().map(str::to_string))
        .collect();
    assert!(
        members.iter().all(|m| m.contains('*')),
        "members должен состоять из масок (src/crates/*, src/microservices/*), а не из списка крейтов: {members:?}"
    );
}

#[test]
fn package_fields_come_from_the_workspace() {
    let _guard = serial();
    let meta = metadata(&project());
    for (name, _) in all_members() {
        let pkg = package(&meta, name);
        assert_eq!(pkg["edition"], "2024", "{name}: edition из [workspace.package]");
        assert_eq!(pkg["license"], "MIT", "{name}: license из [workspace.package]");
        assert_eq!(pkg["version"], "0.1.0", "{name}: version из [workspace.package]");
    }
}

#[test]
fn every_dependency_comes_from_workspace_dependencies() {
    let _guard = serial();
    let root = read_toml(&project().join("Cargo.toml"));
    let shared = root["workspace"]["dependencies"].as_table().expect("нет [workspace.dependencies]");
    for key in ["serde_json", "contracts", "text-engine", "store-engine"] {
        assert!(shared.contains_key(key), "в [workspace.dependencies] не хватает {key}");
    }
    for (name, dir) in all_members() {
        let manifest = read_toml(&project().join(dir).join("Cargo.toml"));
        for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
            let Some(table) = manifest.get(section).and_then(|t| t.as_table()) else {
                continue;
            };
            for (dep, spec) in table {
                assert!(
                    spec.get("workspace").and_then(|v| v.as_bool()) == Some(true),
                    "{name}: зависимость {dep} должна быть записана через workspace = true"
                );
            }
        }
    }
}

#[test]
fn arrows_point_down() {
    let _guard = serial();
    let meta = metadata(&project());
    assert!(dep_names(package(&meta, "contracts"), None).iter().all(|d| d == "serde_json"), "contracts зависит только от serde_json");
    for engine in ["text-engine", "store-engine"] {
        let deps = dep_names(package(&meta, engine), None);
        assert_eq!(deps, ["contracts"], "{engine} зависит только от contracts, а не от соседнего движка и не от сервисов");
    }
    for (service, _) in SERVICES {
        let deps = dep_names(package(&meta, service), None);
        for forbidden in ["greeter-service", "archive-service"] {
            assert!(!deps.contains(&forbidden.to_string()), "{service}: сервисы не зависят друг от друга");
        }
    }
}

#[test]
fn lints_forbid_unsafe_in_every_member() {
    let _guard = serial();
    for (name, dir) in all_members() {
        let sb = sandbox(&format!("mono_unsafe_{name}"));
        let file = if SERVICES.iter().any(|(n, _)| *n == name) { "src/main.rs" } else { "src/lib.rs" };
        let path = sb.join(dir).join(file);
        let mut text = fs::read_to_string(&path).expect("нет исходника");
        text.push_str("\npub fn sneaky() -> u8 {\n    unsafe { 1 }\n}\n");
        fs::write(&path, text).expect("не удалось записать исходник");
        let o = cargo(&sb, &["check", "-p", name]);
        let text = out(&o);
        assert!(
            !o.status.success() && text.contains("unsafe"),
            "{name}: unsafe должен быть запрещён: [workspace.lints] в корне и [lints] workspace = true в {name}.\n{text}"
        );
    }
}

#[test]
fn lints_deny_unwrap_everywhere() {
    let _guard = serial();
    for (name, dir) in all_members() {
        let sb = sandbox(&format!("mono_unwrap_{name}"));
        let file = if SERVICES.iter().any(|(n, _)| *n == name) { "src/main.rs" } else { "src/lib.rs" };
        let path = sb.join(dir).join(file);
        let mut text = fs::read_to_string(&path).expect("нет исходника");
        text.push_str("
pub fn sneaky(x: Option<u8>) -> u8 {
    x.unwrap()
}
");
        fs::write(&path, text).expect("не удалось записать исходник");
        let o = cargo(&sb, &["clippy", "-p", name]);
        let text = out(&o);
        assert!(
            !o.status.success() && text.contains("unwrap_used"),
            "{name}: clippy должен запрещать unwrap: clippy::unwrap_used = \"deny\" в [workspace.lints.clippy] и [lints] workspace = true в {name}.
{text}"
        );
    }
}

#[test]
fn engines_expose_a_facade_and_hide_their_insides() {
    let _guard = serial();
    let probes: [(&str, &str, &str, &str); 3] = [
        ("text-engine", "text_engine", "use text_engine::lexer::split_words;\n#[test]\nfn deep() { let _ = split_words(\"a\"); }\n", "use text_engine::{mentions, tokenize};\n#[test]\nfn flat() { assert_eq!(tokenize(\"a b\").len(), 2); assert_eq!(mentions(\"@x\").len(), 1); }\n"),
        ("store-engine", "store_engine", "use store_engine::index::AuthorIndex;\n#[test]\nfn deep() { let _ = AuthorIndex::default(); }\n", "use store_engine::Store;\n#[test]\nfn flat() { assert!(Store::new().is_empty()); }\n"),
        ("contracts", "contracts", "use contracts::message::Message;\n#[test]\nfn deep() { let _ = Message::from_json(\"{}\"); }\n", "use contracts::{Message, UserId};\n#[test]\nfn flat() { assert_eq!(Message::new(UserId(1), \"a\").from, UserId(1)); }\n"),
    ];
    for (pkg, _, deep, flat) in probes {
        let dir = ENGINES.iter().find(|(n, _)| *n == pkg).map(|(_, d)| *d).expect("известный крейт");
        let sb = sandbox(&format!("mono_facade_{pkg}"));
        fs::create_dir_all(sb.join(dir).join("tests")).expect("не удалось создать tests");
        fs::write(sb.join(dir).join("tests/flat.rs"), flat).expect("не удалось записать пробу");
        let o = cargo(&sb, &["test", "-p", pkg, "--test", "flat"]);
        must_succeed(&o, &format!("проба фасада {pkg}"));
        fs::remove_file(sb.join(dir).join("tests/flat.rs")).expect("не удалось удалить пробу");
        fs::write(sb.join(dir).join("tests/deep.rs"), deep).expect("не удалось записать пробу");
        let o = cargo(&sb, &["test", "-p", pkg, "--test", "deep"]);
        assert!(
            !o.status.success(),
            "{pkg}: внутренний модуль виден снаружи. Модули должны быть приватными (mod), а наружу выходят только pub use в lib.rs"
        );
    }
}

#[test]
fn modules_are_folders_with_mod_rs() {
    let _guard = serial();
    for (name, dir) in all_members() {
        let mut problems = Vec::new();
        layout_problems(&project().join(dir).join("src"), &mut problems);
        assert!(problems.is_empty(), "{name}: раскладка модулей неединая:\n{}", problems.join("\n"));
    }
}

#[test]
fn greeter_service_runs_by_package_name() {
    let _guard = serial();
    let o = cargo(&project(), &["run", "--quiet", "-p", "greeter-service", "--", "hi @ann and @bob"]);
    let text = must_succeed(&o, "cargo run -p greeter-service");
    assert!(text.contains("greeting: ann, bob"), "greeter-service должен печатать `greeting: ann, bob`, а напечатал:\n{text}");
}

#[test]
fn archive_service_counts_messages() {
    let _guard = serial();
    let mut cmd = Command::new(cargo_bin());
    cmd.args(["run", "--quiet", "-p", "archive-service", "--target-dir"]).arg(shared_target()).current_dir(project());
    let o = exec(cmd, Some(b"1: hello\n2: hi\n1: again\nbroken line\n"));
    let text = must_succeed(&o, "cargo run -p archive-service");
    assert!(
        text.contains("{\"authors\":2,\"stored\":3}") || text.contains("{\"stored\":3,\"authors\":2}"),
        "archive-service должен напечатать JSON с stored=3 и authors=2, а напечатал:\n{text}"
    );
}

#[test]
fn one_lock_one_target_one_serde_json() {
    let _guard = serial();
    let sb = sandbox("mono_single");
    must_succeed(&cargo_local(&sb, &["build", "--workspace"]), "cargo build --workspace");
    for (_, dir) in SERVICES {
        must_succeed(&cargo_local(&sb.join(dir), &["build"]), &format!("cargo build внутри {dir}"));
    }
    let mut locks = Vec::new();
    files_named(&sb, "Cargo.lock", &mut locks);
    assert_eq!(locks.len(), 1, "Cargo.lock должен быть один, в корне, а нашлось: {locks:?}");
    let mut targets = Vec::new();
    dirs_named(&sb, "target", &mut targets);
    assert_eq!(targets, [sb.join("target")], "target должен быть один, в корне, а нашлось: {targets:?}");
    let lock = fs::read_to_string(sb.join("Cargo.lock")).expect("нет Cargo.lock");
    assert_eq!(lock.matches("name = \"serde_json\"").count(), 1, "serde_json должен быть в единственной версии");
}

#[test]
fn whole_workspace_tests_pass() {
    let _guard = serial();
    must_succeed(&cargo(&project(), &["test", "--workspace"]), "cargo test --workspace");
}
