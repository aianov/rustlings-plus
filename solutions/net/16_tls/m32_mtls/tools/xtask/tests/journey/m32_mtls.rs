use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn sandbox(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("m32")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("временная папка теста создаётся");
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("tools/xtask лежит на два уровня ниже корня");
    fs::copy(root.join("Cargo.toml"), dir.join("Cargo.toml")).expect("корень копируется");
    dir
}

fn xtask(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::null())
        .output()
        .expect("xtask запускается")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[test]
fn certs_issues_a_ca_and_every_service() {
    let dir = sandbox("issue");
    let out = xtask(&dir, &["certs"]);
    assert!(
        out.status.success(),
        "cargo xtask certs проходит:\n{}",
        text(&out.stderr)
    );
    for name in [
        "ca",
        "api-gateway",
        "auth-service",
        "user-service",
        "post-service",
        "chat-service",
    ] {
        for ext in ["pem", "key"] {
            assert!(
                dir.join("certs").join(format!("{name}.{ext}")).exists(),
                "в certs/ есть {name}.{ext}"
            );
        }
    }
    let ignore = fs::read_to_string(dir.join(".gitignore")).expect(".gitignore появился");
    assert!(
        ignore.lines().any(|line| line.trim() == "certs/"),
        "certs/ в .gitignore: ключи не уходят в репозиторий:\n{ignore}"
    );
    let ca = fs::read(dir.join("certs/ca.pem")).expect("ca.pem");
    let again = xtask(&dir, &["certs"]);
    assert!(
        !again.status.success() && text(&again.stderr).contains("--force"),
        "второй выпуск без --force отказывает и подсказывает флаг:\n{}",
        text(&again.stderr)
    );
    let keep = xtask(&dir, &["certs", "--keep"]);
    assert!(
        keep.status.success(),
        "--keep спокойно оставляет готовые сертификаты"
    );
    assert_eq!(
        fs::read(dir.join("certs/ca.pem")).expect("ca.pem"),
        ca,
        "CA прежний"
    );
    let force = xtask(&dir, &["certs", "--force"]);
    assert!(force.status.success(), "--force перевыпускает");
    assert_ne!(
        fs::read(dir.join("certs/ca.pem")).expect("ca.pem"),
        ca,
        "CA новый"
    );
    assert_eq!(
        fs::read_to_string(dir.join(".gitignore"))
            .expect(".gitignore")
            .matches("certs/")
            .count(),
        1,
        "строка в .gitignore одна, сколько ни запускай"
    );
    let odd = xtask(&dir, &["certs", "--sometimes"]);
    assert!(!odd.status.success(), "незнакомый флаг это ошибка");
}
