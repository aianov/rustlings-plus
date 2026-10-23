use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn clippy(dir: &Path, target: &str) -> Output {
    Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .args(["clippy", "--quiet", "--target-dir"])
        .arg(Path::new(env!("CARGO_TARGET_TMPDIR")).join(target))
        .args(["--", "-D", "warnings"])
        .current_dir(dir)
        .output()
        .expect("не удалось запустить cargo clippy")
}

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("не удалось создать папку");
    for entry in fs::read_dir(from).expect("не удалось прочитать папку") {
        let entry = entry.expect("ошибка чтения папки");
        let name = entry.file_name();
        if name == "target" {
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

fn mutant(name: &str, extra_code: &str) -> Output {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("mutant_{name}"));
    let _ = fs::remove_dir_all(&dir);
    copy_tree(&root(), &dir);
    let lib = dir.join("src/lib.rs");
    let mut code = fs::read_to_string(&lib).expect("нет src/lib.rs");
    code.push_str(extra_code);
    fs::write(&lib, code).expect("не удалось записать src/lib.rs");
    clippy(&dir, &format!("mutant_target_{name}"))
}

#[test]
fn library_has_no_unsafe_and_no_unwrap() {
    let out = clippy(&root(), "lints_self");
    assert!(
        out.status.success(),
        "clippy недоволен кодом пакета:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn unwrap_is_denied_for_new_code() {
    let out = mutant(
        "unwrap",
        "\npub fn added_later(x: Option<u8>) -> u8 {\n    x.unwrap()\n}\n",
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success() && stderr.contains("unwrap_used"),
        "новый .unwrap() должен ронять clippy: добавь clippy::unwrap_used = \"deny\" в [lints.clippy]. Вывод:\n{stderr}"
    );
}

#[test]
fn unsafe_is_forbidden_for_new_code() {
    let out = mutant("unsafe", "\npub fn added_later() -> u8 {\n    unsafe { 1 }\n}\n");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success() && stderr.contains("unsafe"),
        "новый unsafe-блок должен ломать сборку: добавь unsafe_code = \"forbid\" в [lints.rust]. Вывод:\n{stderr}"
    );
}

#[test]
fn first_byte_handles_empty_text() {
    assert_eq!(slug_kit::first_byte("abc"), Some(b'a'), "первый байт строки abc это 'a'");
    assert_eq!(slug_kit::first_byte(""), None, "в пустой строке нет первого байта");
}

#[test]
fn parse_port_returns_none_on_garbage() {
    assert_eq!(slug_kit::parse_port(" 8080 "), Some(8080), "порт 8080 с пробелами по краям");
    assert_eq!(slug_kit::parse_port("http"), None, "слово не порт");
    assert_eq!(slug_kit::parse_port("70000"), None, "70000 не помещается в u16");
}

#[test]
fn slug_still_works() {
    assert_eq!(slug_kit::slug("  Hello Big   World "), "hello-big-world", "slug склеивает слова через дефис");
}
